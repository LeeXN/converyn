//! 控制端装配、公共路由和统一共识写入口；具体处理按职责分组。
mod access;
use access::*;
mod artifacts;
use artifacts::*;
mod secrets;
use secrets::*;
mod outputs;
use outputs::*;
mod workflows;
use workflows::*;
mod remote_logs;
use remote_logs::*;
mod queries;
use queries::*;
mod product;
use product::*;
mod diagnostics;
use diagnostics::*;
mod authentication;
use authentication::*;
mod raft;
use raft::*;
mod runs;
use runs::*;
mod reconciliation;
use reconciliation::*;
mod identity;
use identity::*;
mod administration;
use administration::*;
mod plugin_jobs;
use plugin_jobs::*;

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as ApiPath, Query, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use openraft::{
    BasicNode, Config, SnapshotPolicy,
    raft::{AppendEntriesRequest, InstallSnapshotRequest, VoteRequest},
};
use serde_json::json;

use crate::Raft;
use crate::TypeConfig;
use crate::controller::network::Network;
use crate::storage::store::Store;
use converyn_core::orchestration::machine::Command;

tokio::task_local! {static REQUEST_AUTH:converyn_core::identities::registration::Authorization;}
fn authorized(command: Command, peer_fingerprint: &str) -> Command {
    // Background controller work has no HTTP request scope. Journal its time as
    // peer work too, so plugin completion follows the same replicated transition
    // as Agent reports without consulting a clock inside the state machine.
    let authorization = REQUEST_AUTH.try_with(Clone::clone).unwrap_or_else(|_| {
        converyn_core::identities::registration::Authorization {
            principal: "peer".into(),
            fingerprint: peer_fingerprint.into(),
            token_sha256: None,
            observed_ms: Some(crate::executors::host::now_ms()),
            request_id: None,
        }
    });
    Command::Authorized {
        authorization,
        command: Box::new(command),
    }
}

#[derive(Clone)]
struct App {
    raft: Raft,
    store: Store,
    token: String,
    auth: Option<crate::credentials::security::Auth>,
    issuer: Option<crate::credentials::pki::Authority>,
    output_keyring: Option<std::path::PathBuf>,
    plugin_connections: std::collections::BTreeMap<String, crate::plugins::ConnectionGrant>,
    https: bool,
    client_verifier: Option<Arc<dyn rustls::server::danger::ClientCertVerifier>>,
    cluster: String,
    id: u64,
    addr: String,
    peer_http: reqwest::Client,
    peer_fingerprint: String,
    read_barrier: Arc<ReadBarrier>,
    plugin_admission: Arc<tokio::sync::Semaphore>,
    log_admission: Arc<tokio::sync::Semaphore>,
    logs: Arc<std::sync::RwLock<Option<crate::log_store::Store>>>,
    log_config: Option<crate::log_store::Config>,
    log_probe: String,
    artifact_root: std::path::PathBuf,
    artifact_admission: Arc<tokio::sync::Semaphore>,
    artifact_recovery: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct ReadBarrier {
    next: AtomicU64,
    proof: tokio::sync::Mutex<Option<u64>>,
}

pub async fn serve(
    id: u64,
    listen: SocketAddr,
    cluster: String,
    token: String,
    data_dir: &Path,
) -> Result<()> {
    serve_config(id, listen, cluster, token, data_dir, None, None).await
}

pub async fn serve_config(
    id: u64,
    listen: SocketAddr,
    cluster: String,
    token: String,
    data_dir: &Path,
    auth: Option<crate::credentials::security::Auth>,
    tls: Option<crate::credentials::security::Tls>,
) -> Result<()> {
    serve_settings(Settings {
        log_store: None,
        id,
        listen,
        cluster,
        token,
        data_dir: data_dir.to_path_buf(),
        auth,
        tls,
        issuer: None,
        output_keyring: None,
        plugin_connections: Default::default(),
    })
    .await
}
pub struct Settings {
    pub log_store: Option<crate::log_store::Config>,
    pub id: u64,
    pub listen: SocketAddr,
    pub cluster: String,
    pub token: String,
    pub data_dir: std::path::PathBuf,
    pub auth: Option<crate::credentials::security::Auth>,
    pub tls: Option<crate::credentials::security::Tls>,
    pub issuer: Option<crate::credentials::pki::Authority>,
    pub output_keyring: Option<std::path::PathBuf>,
    pub plugin_connections: std::collections::BTreeMap<String, crate::plugins::ConnectionGrant>,
}
pub async fn serve_settings(settings: Settings) -> Result<()> {
    let Settings {
        id,
        listen,
        cluster,
        token,
        data_dir,
        auth,
        tls,
        issuer,
        output_keyring,
        plugin_connections,
        log_store: log_config,
    } = settings;
    if let Some(authority) = &issuer {
        authority.validate()?;
        ensure!(
            tls.is_some(),
            "online certificate issuance requires mTLS server mode"
        );
    }
    let https = tls.is_some();
    let peer_fingerprint = tls
        .as_ref()
        .map(|tls| crate::credentials::pki::info(&std::fs::read_to_string(&tls.cert)?))
        .transpose()?
        .map(|info| info.fingerprint)
        .unwrap_or_default();
    let client_verifier = tls
        .as_ref()
        .map(crate::credentials::security::Tls::client_verifier)
        .transpose()?;
    if let Some(a) = &auth {
        a.validate()?;
        if https {
            a.validate_bindings()?;
        }
    }
    ensure!(
        !https || auth.is_some(),
        "TLS mode requires separate role credentials"
    );
    // P0 deliberately cannot expose the test/admin protocol on an external interface.
    ensure!(
        listen.ip().is_loopback() || https,
        "non-loopback listeners require mTLS and separate role credentials"
    );
    ensure!(
        id > 0 && converyn_core::model::domain::valid_id(&cluster),
        "invalid node or cluster identity"
    );
    ensure!(
        auth.is_some() || (token.len() >= 32 && token.is_ascii() && !token.contains(['\n', '\r'])),
        "CONVERYN_TOKEN must contain at least 32 ASCII characters"
    );
    let logs = Arc::new(std::sync::RwLock::new(None));
    let log_probe = crate::credentials::pki::random_token()?;
    let store = Store::open(&data_dir, id)?;
    store.bind_cluster(&cluster).await?;
    {
        let machine = store.machine().await?;
        if !machine.state.administration.secrets.is_empty() {
            let keys = crate::credentials::outputs::Keyring::load(
                output_keyring
                    .as_ref()
                    .context("stored Secrets require the original encryption keyring")?,
            )?;
            for (name, versions) in &machine.state.administration.secrets {
                for (version, sealed) in versions {
                    crate::credentials::secrets::open(&keys, name, version, sealed)?;
                }
            }
        }
    }
    let config = Config {
        cluster_name: cluster.clone(),
        heartbeat_interval: 200,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        snapshot_policy: SnapshotPolicy::LogsSinceLast(1000),
        max_in_snapshot_log_to_keep: 0,
        purge_batch_size: 1,
        max_payload_entries: 1,
        snapshot_max_chunk_size: 256 * 1024,
        ..Default::default()
    }
    .validate()?;
    let network = Network {
        store: Some(store.clone()),
        peer_fingerprints: auth
            .as_ref()
            .and_then(|auth| auth.certificates.get("peer"))
            .cloned()
            .unwrap_or_default(),
        https,
        token: auth
            .as_ref()
            .map(|a| a.peer.clone())
            .unwrap_or_else(|| token.clone()),
        cluster: cluster.clone(),
        client: if let Some(tls) = &tls {
            tls.client()?
        } else {
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()?
        },
    };
    let peer_http = network.client.clone();
    let inner = openraft::Raft::<TypeConfig>::new(
        id,
        Arc::new(config),
        network,
        store.clone(),
        store.clone(),
    )
    .await?;
    let raft = Raft::with_store(inner, store.clone());
    let controller_data = data_dir.clone();
    let app = App {
        artifact_root: data_dir.join("artifacts"),
        artifact_admission: Arc::new(tokio::sync::Semaphore::new(4)),
        artifact_recovery: Arc::new(tokio::sync::Mutex::new(())),
        raft: raft.clone(),
        store,
        token,
        auth,
        issuer,
        output_keyring,
        plugin_connections,
        https,
        client_verifier,
        cluster,
        id,
        addr: listen.to_string(),
        peer_http,
        peer_fingerprint,
        read_barrier: Arc::new(ReadBarrier::default()),
        plugin_admission: Arc::new(tokio::sync::Semaphore::new(16)),
        log_admission: Arc::new(tokio::sync::Semaphore::new(16)),
        logs,
        log_config,
        log_probe,
    };
    let backend_controller = tokio::spawn(log_backend(app.clone()));
    let logs_controller = tokio::spawn(log_retention(app.clone()));
    let controller = tokio::spawn(reconcile_loop(app.clone()));
    let history_app = app.clone();
    let history_controller = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if history_app.raft.metrics().borrow().current_leader != Some(history_app.id) {
                continue;
            }
            for _ in 0..32 {
                let Ok(m) = history_app.store.machine().await else {
                    break;
                };
                let Some(change) = history_candidate(&m.state) else {
                    break;
                };
                match history_app
                    .raft
                    .client_write(Command::History { change })
                    .await
                {
                    Ok(r) if r.data.ok => {}
                    _ => break,
                }
            }
        }
    });
    let member_app = app.clone();
    let member_controller = tokio::spawn(async move { member_loop(member_app).await });
    let output_controller = tokio::spawn(materialize_loop(app.clone()));
    let plugin_app = app.clone();
    let plugin_controller =
        tokio::spawn(async move { controller_plugins(plugin_app, controller_data).await });
    let protected = Router::new()
        .route("/api/v1/secrets", get(secret_catalog).post(register_secret))
        .route("/api/v1/artifacts", get(artifact_catalog))
        .route(
            "/api/v1/artifacts/{sha}/chunks",
            post(upload_artifact_chunk),
        )
        .route("/api/v1/artifacts/{sha}/complete", post(complete_artifact))
        .route(
            "/internal/raft/artifacts/{sha}/chunks",
            post(peer_artifact_chunk),
        )
        .route(
            "/internal/raft/artifacts/{sha}/complete",
            post(peer_artifact_complete),
        )
        .route("/internal/raft/artifacts/{sha}", get(peer_artifact_read))
        .route(
            "/api/v1/agents/{node}/runs/{run}/work/{work}/artifacts/{sha}",
            get(work_artifact),
        )
        .route("/api/v1/connections", get(connections))
        .route("/api/v1/access", get(access_list).post(access_write))
        .route("/api/v1/access/me", get(access_me))
        .route("/api/v1/access/check", get(access_check))
        .route(
            "/api/v1/agents/{node}/runs/{run}/work/{work}/inputs",
            get(work_inputs),
        )
        .route("/api/v1/workflows", get(workflows))
        .route("/api/v1/workflows/register", post(register_workflow))
        .route("/api/v1/workflows/{id}", get(workflow_detail))
        .route("/api/v1/workflows/{id}/enabled", post(enable_workflow))
        .route("/api/v1/workflows/{id}/preview", post(workflow_preview))
        .route("/api/v1/workflows/{id}/runs", post(execute_workflow))
        .route("/api/v1/executions", get(workflow_executions))
        .route(
            "/api/v1/identities",
            get(personnel_list).post(personnel_change),
        )
        .route("/api/v1/credentials/{id}", get(credential_detail))
        .route(
            "/api/v1/credentials",
            get(personal_credentials).post(personnel_change),
        )
        .route("/api/v1/agents/{node}/log-storage", get(log_storage_status))
        .route(
            "/api/v1/agents/{node}/logs/{run}/{work}/{stream}",
            post(upload_log),
        )
        .route(
            "/api/v1/agents/{node}/logs/{run}/{work}/{stream}/live",
            post(upload_live),
        )
        .route("/api/v1/runs/{run}/logs/{work}/{stream}", get(read_log))
        .route(
            "/api/v1/runs/{run}/logs/{work}/{stream}/events",
            get(log_events),
        )
        .route(
            "/api/v1/runs/{run}/logs/{work}/{stream}/download",
            get(download_log),
        )
        .route("/api/v1/identity/me", get(me))
        .route("/api/v1/audit", get(audit))
        .route("/api/v1/controlplane/prometheus", get(metrics_prometheus))
        .route("/internal/raft/append", post(append))
        .route("/internal/raft/vote", post(vote))
        .route("/internal/raft/snapshot", post(snapshot))
        .route("/api/v1/controlplane/init", post(initialize))
        .route("/api/v1/controlplane/metrics", get(metrics))
        .route("/api/v1/controlplane/snapshot", post(trigger_snapshot))
        .route("/api/v1/controlplane/backup", get(backup))
        .route(
            "/api/v1/controlplane/members",
            get(member_status).post(replace_member),
        )
        .route("/api/v1/runs/{id}", get(run))
        .route(
            "/api/v1/runs/{id}/work/{work}/definition",
            get(work_definition),
        )
        .route("/api/v1/runs", get(list_runs))
        .route("/api/v1/plugins", get(plugin_catalog))
        .route("/api/v1/targets", get(targets))
        .route("/api/v1/targets/commands", post(node_group_command))
        .route(
            "/api/v1/history",
            get(history_catalog).post(history_command),
        )
        .route("/api/v1/runs/{id}/cancel", post(cancel_run))
        .route("/api/v1/runs/{id}/pause", post(pause_run))
        .route("/api/v1/runs/{id}/resolve", post(resolve_run))
        .route("/api/v1/runs/{id}/uncertain", post(mark_uncertain))
        .route("/api/v1/nodes", get(nodes))
        .route("/api/v1/nodes/{id}", get(node))
        .route("/api/v1/identity/bootstrap", post(issue_bootstrap))
        .route("/api/v1/identity/enroll", post(enroll_node))
        .route("/api/v1/identity/nodes", get(registered_nodes))
        .route("/api/v1/identity/retire", post(retire_node))
        .route("/api/v1/agents/{node}/renew", post(renew_node))
        .route(
            "/api/v1/agents/{node}/renew/activate",
            post(activate_renewal),
        )
        .route("/api/v1/node-groups", get(node_groups))
        .route("/api/v1/node-groups/commands", post(node_group_command))
        .route(
            "/api/v1/security/certificates",
            post(certificate_binding).get(certificate_bindings),
        )
        .route("/api/v1/deployments/{id}", get(deployment))
        .route("/api/v1/deployments/{id}/drift", get(drift_status))
        .route("/api/v1/agents/{node}/heartbeat", post(heartbeat))
        .route("/api/v1/agents/{node}/work", get(poll_work))
        .route("/api/v1/agents/{node}/commands", post(agent_command))
        .route(
            "/api/v1/agents/{node}/attempts/{run}/{work}",
            get(agent_attempt),
        )
        .route("/api/v1/agents/{node}/scripts/{sha}", get(agent_script))
        .route("/health/ready", get(ready))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .route_layer(middleware::from_fn_with_state(app.clone(), authenticate));
    let protected = if !https {
        protected.merge(
            Router::new()
                .route("/api/v1/prototype/commands", post(command))
                .route("/api/v1/prototype/agents/{node}/work", get(poll_work))
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
                .route_layer(middleware::from_fn_with_state(app.clone(), authenticate)),
        )
    } else {
        protected
    };
    let health_app = app.clone();
    let router =
        protected
            .route("/openapi.json", get(crate::interface::contract::schema))
            .route("/", get(crate::interface::web::index))
            .route("/assets/app.js", get(crate::interface::web::javascript))
            .route("/assets/index.css", get(crate::interface::web::stylesheet))
            .route("/assets/version.json", get(crate::interface::web::version_info))
            .route(
                "/health/live",
                get(move || {let app=health_app.clone();async move {
                    let pristine=app.store.machine().await.is_ok_and(|m|m.last_applied.is_none()&&m.state==converyn_core::orchestration::machine::State::default());
                    Json(json!({"live":true,"stage":if https {"v1-development"} else {"p0"},"protocol":crate::controller::network::PROTOCOL_VERSION,"plugin_manifest":crate::plugins::registry().fingerprint(),"node_id":id,"control_plane":app.cluster,"pristine":pristine,"log_storage":log_store(&app).as_ref().map(|s|json!({"id":s.id,"shared":s.shared(),"probe":app.log_probe}))}))
                }}),
            )
            .with_state(app)
            .layer(middleware::from_fn(trace_request));
    tracing::info!(%listen,id,https,"control plane listening");
    if let Some(tls) = tls {
        let handle = axum_server::Handle::new();
        let shutdown = handle.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.graceful_shutdown(Some(Duration::from_secs(5)));
        });
        axum_server::bind(listen)
            .acceptor(crate::credentials::tls_identity::Acceptor(
                axum_server::tls_rustls::RustlsAcceptor::new(tls.server()?),
            ))
            .handle(handle)
            .serve(router.into_make_service())
            .await?;
    } else {
        let listener = tokio::net::TcpListener::bind(listen).await?;
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
    }
    backend_controller.abort();
    logs_controller.abort();
    controller.abort();
    history_controller.abort();

    member_controller.abort();
    plugin_controller.abort();
    output_controller.abort();
    raft.shutdown().await?;
    Ok(())
}

fn unavailable(a: &App) -> Response {
    let metrics = a.raft.metrics().borrow().clone();
    let leader = metrics
        .current_leader
        .and_then(|id| metrics.membership_config.membership().get_node(&id))
        .map(|n| n.addr.clone());
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error":"leader_or_quorum_unavailable","leader":leader})),
    )
        .into_response()
}

async fn command(State(a): State<App>, Json(c): Json<Command>) -> Response {
    if let Ok(auth) = REQUEST_AUTH.try_with(Clone::clone) {
        if !confirm_read(&a).await {
            return unavailable(&a);
        }
        let Ok(m) = a.store.machine().await else {
            return unavailable(&a);
        };
        if !m.state.access_allows(&auth, &c) {
            return denied("submit");
        }
    }
    if let Command::CreateRun { plan, .. } | Command::ApplyDeployment { plan, .. } = &c
        && let Ok(view) = a.store.machine().await
        && let Err(error) = validate_admission(&a, &view.state, plan)
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"plugin_admission_denied","detail":error.to_string()})),
        )
            .into_response();
    }
    if let Err(error) = crate::plugins::validate_command(&c) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"invalid_plugin_contract","detail":error.to_string()})),
        )
            .into_response();
    }
    if let Command::Report {
        run_id,
        work_id,
        receipt: Some(receipt),
        ..
    } = &c
        && let Some(observation) = receipt.native.as_ref().and_then(|e| e.plugin.as_ref())
    {
        let call = a
            .store
            .load_run(run_id)
            .await
            .ok()
            .flatten()
            .and_then(|run| run.effective_work(work_id).map(|w| w.operation.clone()));
        if !matches!(call,Some(converyn_core::model::domain::Operation::Plugin{call}) if crate::plugins::registry().verify(&call,observation).unwrap_or(false))
        {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error":"invalid_plugin_evidence"})),
            )
                .into_response();
        }
    }

    match a
        .raft
        .client_write(authorized(c, &a.peer_fingerprint))
        .await
    {
        Ok(r) => {
            let status = if r.data.ok {
                StatusCode::OK
            } else if r.data.code == "access_denied_before_commit" {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::CONFLICT
            };
            (status, Json(r.data)).into_response()
        }
        Err(error) => submission_error(&a, error),
    }
}

fn submission_error(a: &App, error: crate::SubmissionError) -> Response {
    match error {
        crate::SubmissionError::Full => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "1")],
            Json(json!({"error":"write_queue_full","code":"write_queue_full","retryable":true,"request_id":request_id()})),
        ).into_response(),
        crate::SubmissionError::Pending => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"submission_outcome_unknown","code":"submission_outcome_unknown","retryable":true,"request_id":request_id(),"message":"Query the original intent before retrying with the same identity"})),
        ).into_response(),
        _ => unavailable(a),
    }
}

async fn ready(State(a): State<App>) -> Response {
    if confirm_read(&a).await {
        Json(json!({"ready":true})).into_response()
    } else {
        unavailable(&a)
    }
}

async fn confirm_read(a: &App) -> bool {
    let ticket = a.read_barrier.next.fetch_add(1, Ordering::SeqCst);
    let confirmed = tokio::time::timeout(Duration::from_secs(3), async {
        let mut proof = a.read_barrier.proof.lock().await;
        // Only reads already issued before the quorum probe share its result. New reads
        // require a new probe, even if the last leader confirmation was very recent.
        if proof.is_some_and(|covered| covered >= ticket) {
            return true;
        }
        let covered = a.read_barrier.next.load(Ordering::SeqCst) - 1;
        if a.raft.ensure_linearizable().await.is_err() {
            return false;
        }
        *proof = Some(covered);
        true
    })
    .await
    .unwrap_or(false);
    if !confirmed {
        return false;
    }
    if let Ok(auth) = REQUEST_AUTH.try_with(Clone::clone) {
        let Ok(view) = a.store.machine().await else {
            return false;
        };
        if view
            .state
            .personnel
            .identities
            .contains_key(&auth.principal)
        {
            return view
                .state
                .personnel
                .caller(&auth, crate::executors::host::now_ms())
                .is_some();
        }
        if view
            .state
            .identities
            .bindings
            .get(&auth.principal)
            .is_some_and(|b| !b.fingerprints.contains(&auth.fingerprint))
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod command_time_tests {
    use super::*;

    #[tokio::test]
    async fn background_completion_persists_dates_without_rewriting_retries() {
        use converyn_core::model::domain::PlanInput;
        let mut state = crate::test_support::state();
        let peer = "a".repeat(64);
        state.identities.bindings.insert(
            "peer".into(),
            converyn_core::identities::identity::Binding {
                revision: 1,
                fingerprints: [peer.clone()].into_iter().collect(),
            },
        );
        let plan = serde_yaml::from_str::<PlanInput>(include_str!(
            "../../../../../scripts/tests/fixtures/plan.yaml"
        ))
        .unwrap()
        .compile()
        .unwrap();
        let created = state.apply(authorized(
            Command::CreateRun {
                output_key: None,
                run_id: "background-time".into(),
                idempotency_key: "background-time".into(),
                plan: Box::new(plan),
            },
            &peer,
        ));
        assert!(created.ok, "{}", created.code);
        let submitted = state.runs["background-time"].submitted_ms.unwrap();
        let cancel = Command::Cancel {
            run_id: "background-time".into(),
        };
        assert_eq!(
            state.apply(authorized(cancel.clone(), "")).code,
            "credential_revoked_before_commit"
        );
        assert!(state.apply(authorized(cancel.clone(), &peer)).ok);
        let finished = state.runs["background-time"].finished_ms.unwrap();
        assert!(submitted > 0 && finished >= submitted);

        // A later scoped request must keep its authenticated timestamp, while a
        // terminal retry must keep the first committed completion date.
        let auth = converyn_core::identities::registration::Authorization {
            principal: "peer".into(),
            fingerprint: peer.clone(),
            token_sha256: None,
            observed_ms: Some(finished + 1000),
            request_id: Some("retry".into()),
        };
        let retry = REQUEST_AUTH
            .scope(auth.clone(), async { authorized(cancel.clone(), "unused") })
            .await;
        let Command::Authorized { authorization, .. } = &retry else {
            panic!("request authorization lost")
        };
        assert_eq!(authorization.observed_ms, auth.observed_ms);
        assert_eq!(authorization.request_id, auth.request_id);
        assert!(state.apply(retry).ok);
        assert_eq!(state.runs["background-time"].submitted_ms, Some(submitted));
        assert_eq!(state.runs["background-time"].finished_ms, Some(finished));
        Arc::make_mut(state.runs.get_mut("background-time").unwrap()).finished_ms = None;
        assert!(state.apply(authorized(cancel, &peer)).ok);
        assert!(state.runs["background-time"].finished_ms.is_none());
    }
}
