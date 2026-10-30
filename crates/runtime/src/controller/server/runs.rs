//! 工作领取、回执与人工处置 API，所有写入经统一授权入口。
use super::*;
use converyn_core::model::domain::Operation;

pub(super) async fn run(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
    _headers: axum::http::HeaderMap,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    match a.store.load_run(&id).await {
        Ok(value) => match value {
            Some(r) => {
                let mut value = serde_json::to_value(&r).unwrap();
                if !request_view_run(&m.state, &id) {
                    return denied("view");
                }
                value["plan"] = public_plan(&r.plan);
                value["actions"] = json!({"control":caller_uid(&m.state).is_none_or(|uid|m.state.can_control_run(&uid,&id)),"retry_inputs":r.can_retry_inputs()});
                value["execution"] =
                    serde_json::to_value(m.state.workflows.executions.get(&id)).unwrap();
                Json(value).into_response()
            }
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"run_not_found"})),
            )
                .into_response(),
        },
        Err(_) => unavailable(&a),
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct Poll {
    after: Option<u64>,
    #[serde(default)]
    wait_ms: u64,
}

pub(super) async fn poll_work(
    State(a): State<App>,
    ApiPath(node): ApiPath<String>,
    Query(poll): Query<Poll>,
) -> Response {
    if !converyn_core::model::domain::valid_id(&node) || poll.wait_ms > 10000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_poll"})),
        )
            .into_response();
    }
    let mut metrics = a.raft.metrics();
    if let Some(after) = poll.after {
        let _ = tokio::time::timeout(Duration::from_millis(poll.wait_ms), async {
            loop {
                let m = metrics.borrow().clone();
                if m.current_leader != Some(a.id)
                    || m.last_applied.is_some_and(|id| id.index > after)
                {
                    break;
                }
                if metrics.changed().await.is_err() {
                    break;
                }
            }
        })
        .await;
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
        Ok(m) => {
            let mut work = Vec::new();
            let maintenance_until = m
                .state
                .administration
                .window_until(&node, Some(crate::executors::host::now_ms()));
            let run_ids = match a.store.node_runs(node.clone()).await {
                Ok(v) => v,
                Err(_) => return unavailable(&a),
            };
            for run in run_ids
                .iter()
                .filter_map(|id| m.state.runs.get(id))
                .filter(|r| !r.status.terminal())
            {
                let download_nodes: std::collections::BTreeSet<_> = run
                    .plan
                    .input
                    .host_nodes()
                    .iter()
                    .filter(|n| {
                        m.state.observations.get(&n.uid).is_some_and(|o| {
                            crate::executors::host::now_ms().saturating_sub(o.observed_ms) < 60000
                        }) && run.plan.work.iter().any(|w| {
                            w.node_uid == n.uid
                                && w.operation.artifact().is_some()
                                && !run.prepared.contains(&w.id)
                        })
                    })
                    .take(run.plan.input.download_parallel)
                    .map(|n| n.uid.clone())
                    .collect();
                for declared in run.plan.work.iter().filter(|w| w.node_uid == node) {
                    let effective = run.effective_work(&declared.id);
                    let w = effective.unwrap_or(declared);
                    let attempt = &run.attempts[&w.id];
                    if attempt.status.terminal() {
                        continue;
                    }
                    let claimable = effective.is_some()
                        && !run.ordered_predecessor_pending(w)
                        && !run.preparing
                        && maintenance_until.is_none()
                        && attempt.status == converyn_core::orchestration::machine::Status::Pending
                        && !run.cancel_requested
                        && !run.pause_requested
                        && run.status
                            != converyn_core::orchestration::machine::Status::NeedsAttention
                        && run.plan.work.iter().filter(|p| p.wave < w.wave).all(|p| {
                            run.attempts[&p.id].status
                                == converyn_core::orchestration::machine::Status::Succeeded
                        });
                    work.push(json!({"run_id":run.id,"output_key":run.output_key,"run_sequence":run.sequence,"run_revision":run.revision,"project":run.plan.input.project,"owner_uid":run.plan.input.owner_uid,"work":w,"attempt":attempt,"claimable":claimable,"prefetch":download_nodes.contains(node.as_str())&&run.preparing&&!run.pause_requested&&!run.cancel_requested&&w.operation.artifact().is_some()&&!run.prepared.contains(&w.id)}));
                }
            }
            Json(json!({"cursor":m.last_applied.map(|l|l.index),"node_uid":node,"work":work,"artifact_pins":m.state.cache_pins(&node),"maintenance_until_ms":maintenance_until}))
                .into_response()
        }
        Err(_) => unavailable(&a),
    }
}

pub(super) async fn agent_command(
    State(a): State<App>,
    ApiPath(node): ApiPath<String>,
    headers: axum::http::HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(mut c): Json<Command>,
) -> Response {
    let authenticated = match node_proof(&a, &node, &headers, &peer.0).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Command::Preparation { change } = &mut c {
        match change {
            converyn_core::orchestration::preparation::Change::Prepared {
                node_uid, proof, ..
            }
            | converyn_core::orchestration::preparation::Change::Failed {
                node_uid, proof, ..
            } if node_uid == &node => *proof = Some(authenticated.clone()),
            _ => return StatusCode::FORBIDDEN.into_response(),
        }
    }
    if let Command::Claim { proof, .. } | Command::Report { proof, .. } = &mut c {
        *proof = Some(authenticated);
    }
    if let Command::Claim { authorized_ms, .. } = &mut c {
        *authorized_ms = Some(crate::executors::host::now_ms());
    }
    if !matches!(&c, Command::Preparation { .. })
        && !matches!(&c, Command::Claim{node_uid,..}|Command::Report{node_uid,..} if node_uid==&node)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"agent_command_denied"})),
        )
            .into_response();
    }
    if matches!(
        &c,
        Command::Report {
            status: converyn_core::orchestration::machine::Status::Succeeded
                | converyn_core::orchestration::machine::Status::Failed
                | converyn_core::orchestration::machine::Status::NeedsAttention,
            receipt: None,
            ..
        }
    ) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"execution_receipt_required"})),
        )
            .into_response();
    }
    command(State(a), Json(c)).await
}

pub(super) async fn agent_attempt(
    State(a): State<App>,
    ApiPath((node, run, work)): ApiPath<(String, String, String)>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    if let Ok(Some(r)) = a.store.load_run(&run).await
        && let Some(assigned) = r.effective_work(&work).filter(|w| w.node_uid == node)
    {
        return Json(json!({"output_key":r.output_key,"work":assigned,"attempt":r.attempts[&work],"run_status":r.status,"project":r.plan.input.project,"owner_uid":r.plan.input.owner_uid,"bundle":r.plan.input.bundle,"inventory":r.plan.input.inventory.as_ref().map(|i| i.restricted(&r.plan.input.hosts.iter().flat_map(|h| h.node_uids.iter().cloned()).collect()))})).into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

pub(super) async fn agent_script(
    State(a): State<App>,
    ApiPath((node, sha)): ApiPath<(String, String)>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    if let Ok(m) = a.store.machine().await {
        let assigned = m.state.runs.values().any(|r| {
            r.plan
                .work
                .iter()
                .filter_map(|w| r.effective_work(&w.id))
                .any(|w| {
                    w.node_uid == node
                        && (w.operation.definition_sha() == Some(sha.as_str())
                            || r.plan.input.bundle.values().any(|file| file.sha256 == sha))
                })
        });
        if assigned && let Ok(Some(content)) = a.store.definition(&sha).await {
            return Json(json!({"sha256":sha,"content":content})).into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

/// Read the content address assigned to this exact historical Work, never the
/// workflow's current revision (which may have changed since execution).
pub(super) async fn work_definition(
    State(a): State<App>,
    ApiPath((run, work)): ApiPath<(String, String)>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    if let Ok(Some(run)) = a.store.load_run(&run).await
        && let Some(work) = run
            .effective_work(&work)
            .or_else(|| run.declared_work(&work))
        && let Some(sha) = work.operation.definition_sha()
        && let Ok(Some(content)) = a.store.definition(sha).await
    {
        let rendered_content = if let Operation::Exec { environment, .. } = &work.operation {
            converyn_core::orchestration::values::render(&content, environment)
        } else {
            content.clone()
        };
        return Json(json!({"sha256":sha,"content":content,"rendered_content":rendered_content}))
            .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Heartbeat {
    #[serde(default)]
    diagnostic: Option<String>,
    #[serde(default)]
    inventory: Option<converyn_core::model::telemetry::Snapshot>,
    #[serde(default)]
    resources: Option<converyn_core::model::telemetry::Snapshot>,
    #[serde(default)]
    inventory_checked_ms: Option<u64>,
    #[serde(default)]
    capabilities: Vec<converyn_plugin_api::Identity>,
    #[serde(default)]
    hardware_sha256: Option<String>,
    boot_id: String,
    session_id: String,
}

pub(super) async fn heartbeat(
    State(a): State<App>,
    ApiPath(node): ApiPath<String>,
    headers: axum::http::HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(h): Json<Heartbeat>,
) -> Response {
    let proof = match node_proof(&a, &node, &headers, &peer.0).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return unavailable(&a);
    };
    if let Ok(m) = a.store.machine().await
        && m.state.identity_current(&node, &proof, true)
        && proof.hardware_sha256 == h.hardware_sha256
        && proof
            .session
            .as_ref()
            .is_none_or(|session| session == &h.session_id)
        && m.state.observations.get(&node).is_some_and(|o| {
            h.inventory
                .as_ref()
                .is_none_or(|v| o.inventory.as_ref() == Some(v))
                && h.resources
                    .as_ref()
                    .is_none_or(|v| o.resources.as_ref() == Some(v))
                && h.inventory_checked_ms
                    .is_none_or(|v| o.inventory_checked_ms == Some(v))
                && o.capabilities == h.capabilities
                && o.boot_id == h.boot_id
                && o.session_id == h.session_id
                && o.hardware_sha256 == h.hardware_sha256
                && (now.as_millis() as u64).saturating_sub(o.observed_ms) < 10000
        })
    {
        return Json(json!({"ok":true,"code":"heartbeat_coalesced","run_id":null,"revision":null}))
            .into_response();
    }
    command(
        State(a),
        Json(Command::ObserveNode {
            diagnostic: h.diagnostic,
            inventory: h.inventory,
            resources: h.resources,
            inventory_checked_ms: h.inventory_checked_ms,
            capabilities: h.capabilities,
            proof: Some(proof),
            node_uid: node,
            boot_id: h.boot_id,
            session_id: h.session_id,
            hardware_sha256: h.hardware_sha256,
            observed_ms: now.as_millis() as u64,
        }),
    )
    .await
}

pub(super) async fn nodes(
    State(a): State<App>,
    _headers: axum::http::HeaderMap,
    Query(q): Query<PageQuery>,
) -> Response {
    let after = match q.after("nodes") {
        Ok(v) => v,
        Err(r) => return r,
    };
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let mut rows = m
        .state
        .observations
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .filter(|(_, o)| {
            q.status.as_deref().is_none_or(|status| match status {
                "online" => crate::executors::host::now_ms().saturating_sub(o.observed_ms) < 60000,
                "offline" => {
                    crate::executors::host::now_ms().saturating_sub(o.observed_ms) >= 60000
                }
                _ => false,
            })
        })
        .take(q.limit + 1)
        .collect::<Vec<_>>();
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let cursor = if more {
        rows.last().map(|(id, _)| q.cursor("nodes", id))
    } else {
        None
    };
    Json(json!({"nodes":rows.into_iter().collect::<BTreeMap<_,_>>(),"next_cursor":cursor}))
        .into_response()
}

pub(super) async fn cancel_run(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
    _headers: axum::http::HeaderMap,
) -> Response {
    command(State(a), Json(Command::Cancel { run_id: id })).await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResolveRequest {
    work_id: String,
    expected_revision: u64,
    status: converyn_core::orchestration::machine::Status,
    actor: String,
    evidence: String,
}

pub(super) async fn resolve_run(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
    Json(mut request): Json<ResolveRequest>,
) -> Response {
    if let Ok(actor) = REQUEST_AUTH.try_with(|a| a.principal.clone()) {
        request.actor = actor;
    }
    command(
        State(a),
        Json(Command::Resolve {
            run_id: id,
            work_id: request.work_id,
            expected_revision: request.expected_revision,
            status: request.status,
            actor: request.actor,
            evidence: request.evidence,
        }),
    )
    .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UncertainRequest {
    work_id: String,
    expected_revision: u64,
    reason: String,
}

pub(super) async fn mark_uncertain(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
    Json(request): Json<UncertainRequest>,
) -> Response {
    if request.reason.trim().is_empty() || request.reason.len() > 4096 {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    command(
        State(a),
        Json(Command::MarkUncertain {
            run_id: id,
            work_id: request.work_id,
            expected_revision: request.expected_revision,
            reason: Some(request.reason),
        }),
    )
    .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PauseRequest {
    paused: bool,
    expected_revision: u64,
}

pub(super) async fn pause_run(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
    _headers: axum::http::HeaderMap,
    Json(request): Json<PauseRequest>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let retry = !request.paused
        && a.store
            .load_run(&id)
            .await
            .ok()
            .flatten()
            .is_some_and(|r| r.status == converyn_core::orchestration::machine::Status::Failed);
    let change = if retry {
        Command::RetryInputs {
            run_id: id,
            expected_revision: request.expected_revision,
        }
    } else {
        Command::PauseRun {
            run_id: id,
            paused: request.paused,
            expected_revision: request.expected_revision,
        }
    };
    command(State(a), Json(change)).await
}

pub(super) type RunQuery = PageQuery;

pub(super) async fn list_runs(
    State(a): State<App>,
    _headers: axum::http::HeaderMap,
    Query(query): Query<RunQuery>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    match run_page(&m.state, &query) {
        Ok(page) => Json(page).into_response(),
        Err(response) => response,
    }
}

fn run_page(
    state: &converyn_core::orchestration::machine::State,
    query: &RunQuery,
) -> Result<serde_json::Value, Response> {
    // The cursor carries the immutable date and tie breakers, so new submissions
    // on the first page do not shift or repeat rows on subsequent pages.
    let context = access_cursor_context(state);
    let kind = format!("runs_newest/{context}");
    let after = query.after(&kind)?;
    let before: Option<(u64, u64, String)> = if after.is_empty() {
        None
    } else {
        Some(serde_json::from_str(&after).map_err(|_| StatusCode::BAD_REQUEST.into_response())?)
    };
    let submitted = |id: &str, recorded: Option<u64>| {
        state
            .workflows
            .executions
            .get(id)
            .map(|e| e.submitted_ms)
            .or(recorded)
            .unwrap_or(0)
    };
    let mut index = std::collections::BTreeSet::new();
    for r in state.history.records.values().filter(|r| {
        r.kind == converyn_core::orchestration::history::Kind::Run
            && status_matches(r.status, &query.status)
    }) {
        index.insert((submitted(&r.uid, r.submitted_ms), r.sequence, r.uid.clone()));
    }
    for r in state
        .runs
        .values()
        .filter(|r| status_matches(Some(r.status), &query.status))
    {
        index.insert((submitted(&r.id, r.submitted_ms), r.sequence, r.id.clone()));
    }
    let mut keys = index
        .iter()
        .rev()
        .filter(|(_, _, id)| {
            request_view_run(state, id)
                && state.workflows.executions.get(id).map_or(
                    query.workflow.is_none() && query.preset.is_none() && query.actor.is_none(),
                    |e| {
                        query
                            .workflow
                            .as_ref()
                            .is_none_or(|w| w == &e.request.workflow_uid)
                            && query.preset.as_ref().is_none_or(|p| p == &e.request.target)
                            && query.actor.as_ref().is_none_or(|a| a == &e.actor.uid)
                    },
                )
        })
        .filter(|key| before.as_ref().is_none_or(|cursor| *key < cursor))
        .take(query.limit + 1)
        .collect::<Vec<_>>();
    let more = keys.len() > query.limit;
    keys.truncate(query.limit);
    let next = if more {
        keys.last()
            .map(|key| query.cursor(&kind, &serde_json::to_string(key).unwrap()))
    } else {
        None
    };
    let rows = keys.into_iter().map(|(date, _, id)| {
        let mut row = if let Some(r) = state.runs.get(id) {
            json!({"id":r.id,"status":r.status,"revision":r.revision,"node_group_uid":r.plan.input.hosts.as_ref().map(|h| &h.node_group_uid),"owner":r.plan.input.owner_uid,"total":r.attempts.len(),"completed":r.attempts.values().filter(|a|a.status.terminal()).count(),"finished_ms":r.finished_ms})
        } else {
            let r = state.history.get(converyn_core::orchestration::history::Kind::Run, id).unwrap();
            json!({"id":r.uid,"status":r.status,"revision":r.revision,"node_group_uid":r.node_group_uid,"owner":r.owner,"total":r.total,"completed":r.completed,"archived":true,"finished_ms":r.finished_ms})
        };
        row["submitted_ms"] = if *date > 0 { json!(date) } else { serde_json::Value::Null };
        row["execution"] = serde_json::to_value(state.workflows.executions.get(id)).unwrap();
        row
    }).collect::<Vec<_>>();
    Ok(json!({"runs":rows,"next_cursor":next}))
}

pub(super) async fn node(State(a): State<App>, ApiPath(id): ApiPath<String>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    match m.state.observations.get(&id) {
        Some(n) => Json(json!({"nodes":{id:n}})).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;
    use converyn_core::orchestration::machine::{Command, State as MachineState};

    fn add(state: &mut MachineState, id: &str, date: Option<u64>) {
        if state.registry.nodes.is_empty() {
            *state = crate::test_support::state();
        }
        let plan = serde_yaml::from_str::<converyn_core::model::domain::PlanInput>(include_str!(
            "../../../../../scripts/tests/fixtures/plan.yaml"
        ))
        .unwrap()
        .compile()
        .unwrap();
        assert!(
            state
                .apply(Command::CreateRun {
                    output_key: None,
                    run_id: id.into(),
                    idempotency_key: id.into(),
                    plan: Box::new(plan)
                })
                .ok
        );
        assert!(state.apply(Command::Cancel { run_id: id.into() }).ok);
        let run = std::sync::Arc::make_mut(state.runs.get_mut(id).unwrap());
        run.submitted_ms = date;
        run.finished_ms = date.map(|ms| ms + 10);
    }
    fn query(after: Option<String>, status: Option<String>) -> RunQuery {
        RunQuery {
            after,
            search: None,
            workflow: None,
            preset: None,
            actor: None,
            limit: 2,
            status,
        }
    }
    fn ids(page: &serde_json::Value) -> Vec<&str> {
        page["runs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn dates_sort_across_live_and_archived_pages_with_stable_ties() {
        let mut state = MachineState::default();
        add(&mut state, "z-old", Some(100));
        add(&mut state, "a-new", Some(300));
        add(&mut state, "middle", Some(200));
        add(&mut state, "a-tie", Some(300));
        let revision = state.runs["a-new"].revision;
        assert!(
            state
                .apply(Command::History {
                    change: converyn_core::orchestration::history::Change::Archive {
                        kind: converyn_core::orchestration::history::Kind::Run,
                        uid: "a-new".into(),
                        expected_revision: revision,
                        now_ms: 9999
                    }
                })
                .ok
        );
        let first = run_page(&state, &query(None, None)).unwrap();
        assert_eq!(ids(&first), ["a-tie", "a-new"]);
        assert_eq!(first["runs"][1]["finished_ms"], 310);
        assert_eq!(first["runs"][1]["archived"], true);
        // A newly submitted task must not move the cursor boundary on an older page.
        add(&mut state, "newest", Some(400));
        let after = first["next_cursor"].as_str().unwrap().to_owned();
        let second = run_page(&state, &query(Some(after.clone()), None)).unwrap();
        assert_eq!(ids(&second), ["middle", "z-old"]);
        assert!(second["next_cursor"].is_null());
        assert!(run_page(&state, &query(Some(after), Some("failed".into()))).is_err());
        assert_eq!(
            ids(&run_page(&state, &query(None, None)).unwrap()),
            ["newest", "a-tie"]
        );
    }

    #[test]
    fn unknown_legacy_dates_stay_empty_and_follow_dated_runs() {
        let mut state = MachineState::default();
        add(&mut state, "dated", Some(100));
        add(&mut state, "legacy", None);
        let page = run_page(&state, &query(None, None)).unwrap();
        assert_eq!(ids(&page), ["dated", "legacy"]);
        assert!(page["runs"][1]["submitted_ms"].is_null());
        assert!(page["runs"][1]["finished_ms"].is_null());
        state.workflows.executions.insert("legacy".into(), serde_json::from_value(json!({
            "request": {"workflow_uid":"demo","workflow_revision":"revision","target":"qa","parameters":{},"request_id":"old","plan_sha256":"plan","target_sha256":"target"},
            "actor": {"uid":"person","name":"Person","fingerprint":"fingerprint","credential_uid":"credential"},
            "parameters":{},"submitted_ms":500,"deployment_uid":null,"system":false
        })).unwrap());
        let restored = run_page(&state, &query(None, None)).unwrap();
        assert_eq!(ids(&restored), ["legacy", "dated"]);
        assert_eq!(restored["runs"][0]["submitted_ms"], 500);
        assert!(restored["runs"][0]["finished_ms"].is_null());
    }
}
