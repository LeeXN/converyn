//! 共识传输、选民管理和安全成员替换。
use super::*;

pub(super) async fn append(
    State(a): State<App>,
    Json(r): Json<AppendEntriesRequest<TypeConfig>>,
) -> impl IntoResponse {
    Json(a.raft.append_entries(r).await)
}

pub(super) async fn vote(
    State(a): State<App>,
    Json(r): Json<VoteRequest<u64>>,
) -> impl IntoResponse {
    Json(a.raft.vote(r).await)
}

pub(super) async fn snapshot(
    State(a): State<App>,
    Json(r): Json<InstallSnapshotRequest<TypeConfig>>,
) -> impl IntoResponse {
    Json(a.raft.install_snapshot(r).await)
}

pub(super) async fn initialize(
    State(a): State<App>,
    Json(nodes): Json<BTreeMap<u64, BasicNode>>,
) -> Response {
    if !matches!(nodes.len(), 1 | 3 | 5)
        || nodes.get(&a.id).is_none_or(|n| n.addr != a.addr)
        || nodes.iter().any(|(id, n)| {
            *id == 0
                || !n
                    .addr
                    .parse::<SocketAddr>()
                    .is_ok_and(|s| s.ip().is_loopback() || (a.https && !s.ip().is_unspecified()))
        })
        || nodes
            .values()
            .map(|n| &n.addr)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != nodes.len()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"one_three_or_five_distinct_members_matching_listener_required"})),
        )
            .into_response();
    }
    if let Err(error) = verify_log_members(&a, &nodes).await {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"code":"log_storage_not_shared","message":error.to_string()})),
        )
            .into_response();
    }
    let existing = a
        .raft
        .metrics()
        .borrow()
        .membership_config
        .membership()
        .clone();
    if existing
        .voter_ids()
        .collect::<std::collections::BTreeSet<_>>()
        == nodes.keys().copied().collect()
        && nodes
            .iter()
            .all(|(id, node)| existing.get_node(id) == Some(node))
    {
        return Json(json!({"initialized":true,"replayed":true})).into_response();
    }
    match a.raft.initialize(nodes).await {
        Ok(()) => Json(json!({"initialized":true})).into_response(),
        Err(_) => (
            StatusCode::CONFLICT,
            Json(json!({"error":"already_initialized_or_invalid_membership"})),
        )
            .into_response(),
    }
}

pub(super) async fn metrics(State(a): State<App>) -> impl IntoResponse {
    Json(
        json!({"consistency":"local","metrics":a.raft.metrics().borrow().clone(),"writes":a.raft.write_diagnostics()}),
    )
}

pub(super) async fn trigger_snapshot(State(a): State<App>) -> Response {
    match a.raft.trigger().snapshot().await {
        Ok(()) => (StatusCode::ACCEPTED, Json(json!({"requested":true}))).into_response(),
        Err(_) => unavailable(&a),
    }
}

pub(super) async fn member_status(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
  Ok(m)=>Json(json!({"membership":m.membership,"config_fingerprint":converyn_core::model::domain::digest(&m.membership),"voters":m.membership.membership().voter_ids().collect::<Vec<_>>(),"operations":m.state.controlplane,"replication":a.raft.metrics().borrow().replication})).into_response(),
  Err(_)=>unavailable(&a),
 }
}

pub(super) async fn probe_replacement(
    a: &App,
    intent: &converyn_core::orchestration::controlplane::Replace,
) -> anyhow::Result<serde_json::Value> {
    use sha2::{Digest, Sha256};
    let response = a
        .peer_http
        .get(format!("https://{}/health/live", intent.address))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| e.without_url())?
        .error_for_status()
        .map_err(|e| e.without_url())?;
    let der = response
        .extensions()
        .get::<reqwest::tls::TlsInfo>()
        .and_then(|v| v.peer_certificate())
        .context("replacement TLS identity missing")?;
    anyhow::ensure!(
        hex::encode(Sha256::digest(der)) == intent.new_fingerprint,
        "replacement TLS fingerprint differs from intent"
    );
    let value: serde_json::Value = response.json().await?;
    anyhow::ensure!(
        value["node_id"] == intent.new_id
            && value["protocol"] == crate::controller::network::PROTOCOL_VERSION
            && value["control_plane"] == a.cluster,
        "replacement identity, cluster or protocol mismatch"
    );
    Ok(value)
}

pub(super) async fn replace_member(
    State(a): State<App>,
    Json(intent): Json<converyn_core::orchestration::controlplane::Replace>,
) -> Response {
    if !a.https {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    if let Some(existing) = m.state.controlplane.operations.get(&intent.uid) {
        return if existing.intent == intent {
            Json(converyn_core::orchestration::machine::Reply {
                ok: true,
                code: "member_operation_updated".into(),
                run_id: None,
                revision: Some(existing.revision),
            })
            .into_response()
        } else {
            StatusCode::CONFLICT.into_response()
        };
    }
    let membership = m.membership.membership();
    let voters = membership
        .voter_ids()
        .collect::<std::collections::BTreeSet<_>>();
    if membership.get_joint_config().len() != 1
        || voters != intent.voters
        || converyn_core::model::domain::digest(&m.membership) != intent.expected_membership
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error":"membership_changed"})),
        )
            .into_response();
    }
    let Ok(address) = intent.address.parse::<SocketAddr>() else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    if address.port() == 0
        || address.ip().is_unspecified()
        || address.ip().is_multicast()
        || membership.nodes().any(|(id, node)| {
            *id != intent.old_id && *id != intent.new_id && node.addr == intent.address
        })
    {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    if membership
        .get_node(&intent.old_id)
        .is_none_or(|node| node.addr != intent.address)
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"v1_replacement_preserves_the_configured_endpoint"})),
        )
            .into_response();
    }
    let probe = probe_replacement(&a, &intent).await;
    if probe.as_ref().is_err()
        || (!membership.nodes().any(|(id, _)| *id == intent.new_id)
            && probe.as_ref().is_ok_and(|v| v["pristine"] != true))
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error":"replacement_not_verified_or_not_pristine"})),
        )
            .into_response();
    }
    let existing_fingerprints = m
        .state
        .identities
        .bindings
        .get("peer")
        .map(|v| v.fingerprints.clone())
        .or_else(|| {
            a.auth
                .as_ref()
                .and_then(|v| v.certificates.get("peer"))
                .cloned()
        })
        .unwrap_or_default();
    command(
        State(a),
        Json(Command::Controlplane {
            change: converyn_core::orchestration::controlplane::Change::Request {
                intent,
                existing_fingerprints,
            },
        }),
    )
    .await
}

pub(super) async fn observe_member(
    a: &App,
    operation: &converyn_core::orchestration::controlplane::Operation,
    condition: &str,
    diagnostic: Option<&str>,
) {
    let diagnostic = diagnostic.map(String::from);
    if operation.condition == condition && operation.diagnostic == diagnostic {
        return;
    }
    let _ = command(
        State(a.clone()),
        Json(Command::Controlplane {
            change: converyn_core::orchestration::controlplane::Change::Observe {
                uid: operation.intent.uid.clone(),
                expected_revision: operation.revision,
                condition: condition.into(),
                diagnostic,
            },
        }),
    )
    .await;
}

pub(super) async fn member_loop(a: App) {
    let mut delay = 1;
    loop {
        tokio::time::sleep(Duration::from_secs(delay)).await;
        if a.raft.metrics().borrow().current_leader != Some(a.id) {
            delay = 1;
            continue;
        }
        let Ok(m) = a.store.machine().await else {
            continue;
        };
        let Some(uid) = &m.state.controlplane.active else {
            delay = 1;
            continue;
        };
        let Some(operation) = m.state.controlplane.operations.get(uid).cloned() else {
            continue;
        };
        let intent = &operation.intent;
        let mut desired = intent.voters.clone();
        desired.remove(&intent.old_id);
        desired.insert(intent.new_id);
        let current = m.membership.membership();
        if current.get_joint_config().len() == 1
            && current
                .voter_ids()
                .collect::<std::collections::BTreeSet<_>>()
                == desired
        {
            if confirm_read(&a).await {
                observe_member(&a, &operation, "succeeded", None).await;
            }
            delay = 1;
            continue;
        }
        if probe_replacement(&a, intent).await.is_err() {
            observe_member(
                &a,
                &operation,
                "waiting_for_member",
                Some("replacement endpoint, certificate or protocol is not ready"),
            )
            .await;
            delay = (delay * 2).min(30);
            continue;
        }
        if current
            .nodes()
            .any(|(id, node)| *id == intent.new_id && node.addr != intent.address)
        {
            observe_member(
                &a,
                &operation,
                "waiting_for_member",
                Some("replacement ID has another address; inspect membership"),
            )
            .await;
            delay = 30;
            continue;
        }
        let node = BasicNode {
            addr: intent.address.clone(),
        };
        let added = tokio::time::timeout(
            Duration::from_secs(90),
            a.raft.add_learner(intent.new_id, node, true),
        )
        .await;
        if !matches!(added, Ok(Ok(_))) {
            observe_member(
                &a,
                &operation,
                "catching_up",
                Some("replacement is not caught up; existing voters are retained"),
            )
            .await;
            delay = (delay * 2).min(30);
            continue;
        }
        let required = m.last_applied.map(|id| id.index).unwrap_or(0);
        let caught_up = a
            .raft
            .metrics()
            .borrow()
            .replication
            .as_ref()
            .and_then(|map| map.get(&intent.new_id))
            .and_then(|log| *log)
            .is_some_and(|log| log.index >= required);
        if !caught_up {
            observe_member(
                &a,
                &operation,
                "catching_up",
                Some("replacement has not acknowledged the committed state"),
            )
            .await;
            delay = 1;
            continue;
        }
        let Ok(current) = a.store.machine().await else {
            continue;
        };
        let Some(active) = current.state.controlplane.operations.get(uid) else {
            continue;
        };
        observe_member(&a, active, "changing_voters", None).await;
        let changed = tokio::time::timeout(
            Duration::from_secs(90),
            a.raft.change_membership(desired, false),
        )
        .await;
        delay = if matches!(changed, Ok(Ok(_))) {
            1
        } else {
            (delay * 2).min(30)
        };
    }
}
