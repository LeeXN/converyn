//! Both interfaces submit workflow identifiers, never executable plans.
use super::*;
use converyn_core::model::domain::Operation;
use converyn_core::{identities::personnel, orchestration::workflow};
use serde::{Deserialize, Serialize};

fn failure(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"error":"workflow_input_rejected","detail":error.to_string(),"message":error.to_string()})),
    )
        .into_response()
}

pub(super) async fn workflows(State(a): State<App>, Query(q): Query<PageQuery>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let context = access_cursor_context(&m.state);
    let kind = format!("workflows/{context}");
    let after = match q.after(&kind) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let mut rows = m
        .state
        .workflows
        .workflows
        .values()
        .filter(|w| {
            w.uid > after
                && q.status
                    .as_deref()
                    .is_none_or(|s| s == "all" || (s == "enabled") == w.enabled)
        })
        .filter_map(|w| workflow_view(&m.state, w, false))
        .filter(|v| {
            q.search.as_ref().is_none_or(|s| {
                format!("{} {}", v["workflow"]["uid"], v["definition"]["name"])
                    .to_lowercase()
                    .contains(&s.to_lowercase())
            })
        })
        .take(q.limit + 1)
        .collect::<Vec<_>>();
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let next = if more {
        rows.last()
            .and_then(|v| v["workflow"]["uid"].as_str())
            .map(|id| q.cursor(&kind, id))
    } else {
        None
    };
    Json(json!({"workflows":rows,"next_cursor":next})).into_response()
}
pub(super) async fn workflow_detail(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let Some(w) = m.state.workflows.workflows.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(mut value) = workflow_view(&m.state, w, true) else {
        return denied("view");
    };
    let definition = &m.state.workflows.revisions[&w.current];
    let reconciliation=definition.targets.iter().filter(|(t,_)|request_allows(&m.state,converyn_core::identities::access::Action::View,Some(&id),Some(t))).filter_map(|(target,preset)| {
        let owner=preset.owner_uid.clone().unwrap_or_else(||format!("workflow-{}",&converyn_core::model::domain::digest(&(&w.uid,target))[..32]));
        m.state.deployments.get(&owner).map(|deployment|json!({"target":target,"deployment":{"condition":deployment.condition,"last_run":deployment.last_run},"policy":m.state.drift.get(&owner)}))
    }).collect::<Vec<_>>();
    value["reconciliation"] = json!(reconciliation);
    Json(value).into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegisterWorkflow {
    workflow: workflow::Revision,
    expected_revision: u64,
}
pub(super) async fn register_workflow(
    State(a): State<App>,
    Json(r): Json<RegisterWorkflow>,
) -> Response {
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    for step in &r.workflow.playbook.steps {
        if matches!(
            &step.operation,
            Operation::Exec {
                execution_profile: Some(_),
                ..
            }
        ) {
            return failure(
                "execution_profile was removed; declare execution: host or user on the step",
            );
        }
        if let Some(artifact) = step.operation.artifact()
            && artifact.locations.is_empty()
            && !r.workflow.parameters.values().any(|p| {
                p.bindings
                    .iter()
                    .any(|b| b.step_id == step.id && b.path.starts_with("/artifact/"))
            })
            && m.state.administration.artifacts.get(&artifact.sha256) != Some(&artifact.size)
        {
            return failure(format!(
                "step {} managed artifact {} is not uploaded; register the workflow with its local artifact files",
                step.id, artifact.sha256
            ));
        }
    }
    command(
        State(a),
        Json(Command::Workflow {
            change: workflow::Change::Register {
                workflow: Box::new(r.workflow),
                expected_revision: r.expected_revision,
            },
        }),
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnableWorkflow {
    enabled: bool,
    expected_revision: u64,
}
pub(super) async fn enable_workflow(
    State(a): State<App>,
    ApiPath(uid): ApiPath<String>,
    Json(r): Json<EnableWorkflow>,
) -> Response {
    command(
        State(a),
        Json(Command::Workflow {
            change: workflow::Change::Enable {
                uid,
                enabled: r.enabled,
                expected_revision: r.expected_revision,
            },
        }),
    )
    .await
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkflowPreview {
    workflow_revision: String,
    target: String,
    #[serde(default)]
    parameters: BTreeMap<String, serde_json::Value>,
    request_id: String,
}
pub(super) async fn workflow_preview(
    State(a): State<App>,
    ApiPath(uid): ApiPath<String>,
    Json(r): Json<WorkflowPreview>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    if !request_allows(
        &m.state,
        converyn_core::identities::access::Action::Preview,
        Some(&uid),
        Some(&r.target),
    ) {
        return denied("preview");
    }
    if !converyn_core::model::domain::valid_id(&r.request_id) || r.request_id.len() > 124 {
        return failure("invalid_request_id");
    }
    let original = format!("run-{}", r.request_id);
    let existing = m.state.workflows.executions.get(&original);
    let compiled = if let Some(execution) = existing {
        // Re-previewing an accepted request must preserve its committed snapshot,
        // including after membership changes. It never grants a new execution.
        if caller_uid(&m.state).as_deref() != Some(execution.actor.uid.as_str())
            || execution.request.workflow_uid != uid
            || execution.request.workflow_revision != r.workflow_revision
            || execution.request.target != r.target
            || execution.request.parameters != r.parameters
        {
            return failure("run_idempotency_conflict");
        }
        match a.store.load_run(&original).await {
            Ok(Some(run)) => Ok((
                run.plan.as_ref().clone(),
                execution.parameters.clone(),
                execution.request.target_sha256.clone(),
            )),
            _ => return failure("accepted_run_snapshot_unavailable"),
        }
    } else {
        m.state.workflow_plan(
            &uid,
            &r.workflow_revision,
            &r.target,
            &r.parameters,
            crate::plugins::registry(),
        )
    };
    match compiled {
        Ok((plan, parameters, target_sha256)) => {
            if existing.is_none()
                && let Err(error) = validate_admission(&a, &m.state, &plan)
            {
                return failure(error);
            }
            let request = workflow::Request {
                workflow_uid: uid,
                workflow_revision: r.workflow_revision,
                target: r.target,
                parameters: r.parameters,
                request_id: r.request_id,
                plan_sha256: plan.sha256.clone(),
                target_sha256,
            };
            let source = &m.state.workflows.revisions[&request.workflow_revision];
            let rendered_scripts: BTreeMap<_, _> = plan
                .work
                .iter()
                .filter_map(|work| {
                    if let Operation::Exec {
                        script_sha256,
                        environment,
                        ..
                    } = &work.operation
                    {
                        source.definitions.get(script_sha256).map(|script| {
                            (
                                work.id.clone(),
                                converyn_core::orchestration::values::render(script, environment),
                            )
                        })
                    } else {
                        None
                    }
                })
                .collect();
            Json(json!({"request":request,"plan":public_plan(&plan),"parameters":parameters,"rendered_scripts":rendered_scripts})).into_response()
        }
        Err(error) => failure(error),
    }
}
pub(super) async fn execute_workflow(
    State(a): State<App>,
    ApiPath(uid): ApiPath<String>,
    Json(request): Json<workflow::Request>,
) -> Response {
    if uid != request.workflow_uid {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let Ok(auth) = REQUEST_AUTH.try_with(Clone::clone) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if !request_allows(
        &m.state,
        converyn_core::identities::access::Action::Execute,
        Some(&uid),
        Some(&request.target),
    ) {
        return denied("execute");
    }
    let now = crate::executors::host::now_ms();
    let Some((identity, credential)) = m.state.personnel.caller(&auth, now) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let actor = workflow::Actor {
        uid: identity.uid.clone(),
        name: identity.name.clone(),
        fingerprint: credential.fingerprint.clone(),
        credential_uid: credential.uid.clone(),
    };
    let original = format!("run-{}", request.request_id);
    // A retry of an accepted request must work even after its workflow changed.
    if !m.state.workflows.executions.contains_key(&original) {
        match m.state.workflow_plan(
            &uid,
            &request.workflow_revision,
            &request.target,
            &request.parameters,
            crate::plugins::registry(),
        ) {
            Ok((plan, _, _)) => {
                if let Err(error) = validate_admission(&a, &m.state, &plan) {
                    return failure(error);
                }
            }
            Err(error) => return failure(error),
        }
    }
    let output_key = match &a.output_keyring {
        Some(path) => match crate::credentials::outputs::Keyring::load(path) {
            Ok(keys) => Some(keys.public_key()),
            Err(_) => return failure("output_encryption_key_unavailable"),
        },
        None => None,
    };
    command(
        State(a),
        Json(Command::Workflow {
            change: workflow::Change::Execute {
                output_key,
                request: Box::new(request),
                actor,
                now_ms: now,
            },
        }),
    )
    .await
}
pub(super) async fn workflow_executions(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
        Ok(m) => Json(json!({"executions":m.state.workflows.executions.iter().filter(|(id,_)|request_view_run(&m.state,id)).collect::<BTreeMap<_,_>>()})).into_response(),
        Err(_) => unavailable(&a),
    }
}
pub(super) async fn personnel_list(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
        Ok(m) => Json(json!({"identities":m.state.personnel.identities})).into_response(),
        Err(_) => unavailable(&a),
    }
}
pub(super) async fn personal_credentials(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
        Ok(m) => {
            let now = crate::executors::host::now_ms();
            let rows=m.state.personnel.credentials.values().map(|c|json!({"uid":c.uid,"identity_uid":c.identity_uid,"identity_name":m.state.personnel.identities.get(&c.identity_uid).map(|i| &i.name),"fingerprint":c.fingerprint,"not_before_ms":c.not_before_ms,"expires_ms":c.expires_ms,"revoked_ms":c.revoked_ms,"overlap_until_ms":c.overlap_until_ms,"active":c.active(now) && m.state.personnel.identities.get(&c.identity_uid).is_some_and(|i| !i.disabled),"serial":crate::credentials::pki::serial(&c.certificate_pem).ok()})).collect::<Vec<_>>();
            Json(json!({"credentials":rows})).into_response()
        }
        Err(_) => unavailable(&a),
    }
}
/// Public material only: local private keys and bearer values never enter the registry.
pub(super) async fn credential_detail(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let Some(c) = m.state.personnel.credentials.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let now = crate::executors::host::now_ms();
    Json(json!({"credential":{"uid":c.uid,"identity_uid":c.identity_uid,"identity_name":m.state.personnel.identities.get(&c.identity_uid).map(|i|&i.name),"fingerprint":c.fingerprint,"not_before_ms":c.not_before_ms,"expires_ms":c.expires_ms,"revoked_ms":c.revoked_ms,"overlap_until_ms":c.overlap_until_ms,"active":c.active(now)&&m.state.personnel.identities.get(&c.identity_uid).is_some_and(|i|!i.disabled),"serial":crate::credentials::pki::serial(&c.certificate_pem).ok(),"certificate_pem":c.certificate_pem},"storage":{"public_certificate":"replicated controller registry (included in logical backups)","private_key":"client-held; not stored by the controller","token":"client-held; only its digest is registered","original_client_directory":"not recorded by the controller"}})).into_response()
}
pub(super) async fn personnel_change(
    State(a): State<App>,
    Json(change): Json<personnel::Change>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let principal = REQUEST_AUTH
        .try_with(|a| a.principal.clone())
        .unwrap_or_default();
    if principal == "admin" {
        let permitted = match &change {
            personnel::Change::Create {
                super_admin: true, ..
            } => m.state.personnel.identities.is_empty(),
            personnel::Change::Issue { credential, .. } => {
                !m.state
                    .personnel
                    .has_active_admin(crate::executors::host::now_ms())
                    && m.state
                        .personnel
                        .identities
                        .get(&credential.identity_uid)
                        .is_some_and(|i| m.state.personnel.super_admin(&i.uid) && !i.disabled)
            }
            _ => false,
        };
        if !permitted {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let now = crate::executors::host::now_ms();
    let committed_time = REQUEST_AUTH
        .try_with(|a| a.observed_ms)
        .ok()
        .flatten()
        .unwrap_or(now);
    if let personnel::Change::Create { credential, .. }
    | personnel::Change::Issue { credential, .. } = &change
    {
        // Reject accidental combined certificate/key PEM before the command can
        // enter Raft, including rejected command history.
        if let Err(error) = credential.validate(now) {
            return failure(error);
        }

        if let Some(verifier) = &a.client_verifier {
            let chain: Result<Vec<_>, _> =
                rustls_pemfile::certs(&mut credential.certificate_pem.as_bytes()).collect();
            let Ok(chain) = chain else {
                return failure("invalid_certificate_chain");
            };
            if chain.is_empty()
                || verifier
                    .verify_client_cert(&chain[0], &chain[1..], rustls::pki_types::UnixTime::now())
                    .is_err()
            {
                return failure("personal_certificate_not_trusted_by_controller");
            }
        }
        if a.auth.as_ref().is_some_and(|auth| {
            std::iter::once(&auth.admin)
                .chain([&auth.peer])
                .chain(auth.agents.values())
                .any(|t| token_hash(t) == credential.token_sha256)
        }) || m
            .state
            .registry
            .token_index
            .contains_key(&credential.token_sha256)
            || m.state
                .registry
                .nodes
                .values()
                .any(|n| n.certificate.fingerprint == credential.fingerprint)
        {
            return failure("credential_role_alias");
        }
    }
    let mut change = change;
    // Never accept caller-provided future/past time as a way to extend credentials.
    match &mut change {
        personnel::Change::Create { now_ms, .. }
        | personnel::Change::Issue { now_ms, .. }
        | personnel::Change::Revoke { now_ms, .. } => {
            *now_ms = committed_time;
        }
        _ => {}
    }
    command(State(a), Json(Command::Personnel { change })).await
}
