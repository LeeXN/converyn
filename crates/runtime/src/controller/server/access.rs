//! Authorization shared by HTTP views and the committed command path.
use super::*;
use converyn_core::identities::access::{Action, Change, Role};
use converyn_core::orchestration::machine::State as BusinessState;

pub(super) fn caller_uid(state: &BusinessState) -> Option<String> {
    REQUEST_AUTH
        .try_with(|a| {
            state
                .personnel
                .caller(a, crate::executors::host::now_ms())
                .map(|(i, _)| i.uid.clone())
        })
        .ok()
        .flatten()
}
pub(super) fn request_allows(
    state: &BusinessState,
    action: Action,
    workflow: Option<&str>,
    preset: Option<&str>,
) -> bool {
    // Unauthenticated lab mode has no personal principal; personal credentials always fail closed.
    REQUEST_AUTH
        .try_with(|a| {
            state
                .personnel
                .caller(a, crate::executors::host::now_ms())
                .is_some_and(|(i, _)| state.personnel.allows(&i.uid, action, workflow, preset))
        })
        .unwrap_or(true)
}
pub(super) fn request_view_run(state: &BusinessState, run: &str) -> bool {
    REQUEST_AUTH
        .try_with(|a| {
            state
                .personnel
                .caller(a, crate::executors::host::now_ms())
                .is_some_and(|(i, _)| state.can_view_run(&i.uid, run))
        })
        .unwrap_or(true)
}
pub(super) fn denied(action: &str) -> Response {
    (StatusCode::FORBIDDEN, Json(json!({"error":"permission_denied","action":action,"message":format!("Permission required: {action}. Ask an access administrator to check your role bindings.")}))).into_response()
}
pub(super) fn personal_path_allowed(
    state: &BusinessState,
    uid: &str,
    method: &axum::http::Method,
    path: &str,
) -> bool {
    let allow = |a| state.personnel.allows(uid, a, None, None);
    if path == "/api/v1/identity/me"
        || path == "/api/v1/access/check"
        || path == "/api/v1/access/me"
    {
        return true;
    }
    if path.starts_with("/api/v1/access")
        || path.starts_with("/api/v1/identities")
        || path.starts_with("/api/v1/credentials")
    {
        return allow(Action::Access);
    }
    if path == "/api/v1/workflows" || path == "/api/v1/runs" || path == "/api/v1/executions" {
        return method == axum::http::Method::GET;
    }
    if path == "/api/v1/workflows/register" {
        return method == axum::http::Method::POST;
    } // body-scoped commit check
    if let Some(rest) = path.strip_prefix("/api/v1/workflows/") {
        let mut parts = rest.split('/');
        let id = parts.next().unwrap_or_default();
        return match parts.next() {
            None if method == axum::http::Method::GET => state
                .workflows
                .workflows
                .get(id)
                .and_then(|w| state.workflows.revisions.get(&w.current))
                .is_some_and(|r| {
                    state
                        .personnel
                        .allows(uid, Action::Maintain, Some(id), None)
                        || r.targets
                            .keys()
                            .any(|t| state.personnel.allows(uid, Action::View, Some(id), Some(t)))
                }),
            Some("preview" | "runs") if method == axum::http::Method::POST => true, // target checked before compilation
            Some("enabled") if method == axum::http::Method::POST => {
                state
                    .personnel
                    .allows(uid, Action::Maintain, Some(id), None)
            }
            _ => false,
        };
    }
    if let Some(rest) = path.strip_prefix("/api/v1/runs/") {
        let mut parts = rest.split('/');
        let id = parts.next().unwrap_or_default();
        return if method == axum::http::Method::GET {
            state.can_view_run(uid, id)
        } else {
            match parts.next() {
                Some("pause" | "cancel") => state.can_control_run(uid, id),
                Some("resolve" | "uncertain") => {
                    state.workflows.executions.get(id).is_some_and(|e| {
                        state.personnel.allows(
                            uid,
                            Action::Control,
                            Some(&e.request.workflow_uid),
                            Some(&e.request.target),
                        )
                    })
                }
                _ => false,
            }
        };
    }
    if method == axum::http::Method::GET
        && path.starts_with("/api/v1/agents/")
        && path.ends_with("/log-storage")
    {
        return allow(Action::Platform);
    }
    // No personal identity may impersonate a peer or an Agent, including super administrators.
    if path.starts_with("/internal/")
        || path.starts_with("/api/v1/agents/")
        || path == "/api/v1/identity/enroll"
    {
        return false;
    }
    if path == "/api/v1/identity/certificates" {
        return state.personnel.super_admin(uid);
    }
    allow(Action::Platform)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccessCheck {
    action: Action,
    workflow: Option<String>,
    preset: Option<String>,
}
pub(super) async fn access_check(State(a): State<App>, Query(q): Query<AccessCheck>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let allowed = request_allows(
        &m.state,
        q.action,
        q.workflow.as_deref(),
        q.preset.as_deref(),
    );
    Json(json!({"allowed":allowed,"action":q.action,"workflow":q.workflow,"preset":q.preset,"reason":if allowed {"matching role binding"} else {"no matching role binding"}})).into_response()
}
pub(super) async fn access_me(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let uid = caller_uid(&m.state).unwrap_or_default();
    let bindings = m.state.personnel.bindings_for(&uid).collect::<Vec<_>>();
    Json(json!({"revision":m.state.personnel.access.revision,"bindings":bindings,"platform":request_allows(&m.state, Action::Platform,None,None),"access":request_allows(&m.state, Action::Access,None,None),"super_admin":m.state.personnel.super_admin(&uid)})).into_response()
}
pub(super) async fn access_list(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    Json(json!({"revision":m.state.personnel.access.revision,"groups":m.state.personnel.access.groups,"bindings":m.state.personnel.access.bindings,"workflows":m.state.workflows.workflows.values().filter_map(|w|m.state.workflows.revisions.get(&w.current).map(|r|json!({"uid":w.uid,"name":r.name,"presets":r.targets.keys().collect::<Vec<_>>()}))).collect::<Vec<_>>(),"roles":[Role::Viewer,Role::Executor,Role::Maintainer,Role::PlatformOperator,Role::AccessAdmin,Role::SuperAdmin]})).into_response()
}
pub(super) async fn access_write(State(a): State<App>, Json(change): Json<Change>) -> Response {
    command(State(a), Json(Command::Access { change })).await
}
pub(super) fn workflow_view(
    state: &BusinessState,
    w: &converyn_core::orchestration::workflow::Workflow,
    full: bool,
) -> Option<serde_json::Value> {
    let mut definition = serde_json::to_value(state.workflows.revisions.get(&w.current)?).ok()?;
    let targets = definition["targets"].as_object_mut()?;
    targets.retain(|preset, _| request_allows(state, Action::View, Some(&w.uid), Some(preset)));
    if targets.is_empty() && !request_allows(state, Action::Maintain, Some(&w.uid), None) {
        return None;
    }
    let actions = json!({"maintain":request_allows(state,Action::Maintain,Some(&w.uid),None),"execute":targets.keys().filter(|p| request_allows(state,Action::Execute,Some(&w.uid),Some(p))).collect::<Vec<_>>()});
    if !full {
        definition.as_object_mut()?.remove("definitions");
    }
    let visible_nodes: std::collections::BTreeSet<_> = state.workflows.revisions[&w.current]
        .targets
        .iter()
        .filter(|(preset, _)| request_allows(state, Action::View, Some(&w.uid), Some(preset)))
        .flat_map(|(_, p)| {
            state.workflows.revisions[&w.current]
                .inventory
                .as_ref()
                .and_then(|i| p.hosts.as_ref().and_then(|hosts| i.select(hosts).ok()))
                .unwrap_or_default()
        })
        .collect();
    if !request_allows(state, Action::Maintain, Some(&w.uid), None)
        && let Some(values) = definition
            .get_mut("node_values")
            .and_then(serde_json::Value::as_object_mut)
    {
        values.retain(|uid, _| visible_nodes.contains(uid));
    }
    // A target grant reveals only that target's workflow topology, never platform membership.
    if !request_allows(state, Action::Maintain, Some(&w.uid), None)
        && let Some(inventory) = definition
            .get_mut("inventory")
            .and_then(serde_json::Value::as_object_mut)
    {
        if let Some(nodes) = inventory
            .get_mut("nodes")
            .and_then(serde_json::Value::as_array_mut)
        {
            nodes.retain(|node| {
                node["uid"]
                    .as_str()
                    .is_some_and(|id| visible_nodes.contains(id))
            });
        }
        if let Some(groups) = inventory
            .get_mut("groups")
            .and_then(serde_json::Value::as_object_mut)
        {
            groups.retain(|_, members| {
                if let Some(ids) = members.as_array_mut() {
                    ids.retain(|id| id.as_str().is_some_and(|id| visible_nodes.contains(id)));
                    !ids.is_empty()
                } else {
                    false
                }
            });
        }
    }
    // Fixed config/provenance may name other presets or platform connections.
    if !request_allows(state, Action::Maintain, Some(&w.uid), None) {
        definition.as_object_mut()?.remove("fixed");
        definition.as_object_mut()?.remove("provenance");
    }
    Some(json!({"workflow":w,"definition":definition,"actions":actions}))
}

pub(super) fn access_cursor_context(state: &BusinessState) -> String {
    let uid = caller_uid(state);
    converyn_core::model::domain::digest(&(
        uid,
        &state.personnel.access,
        &state.personnel.identities,
        &state.workflows.workflows,
        &state.node_groups,
    ))
}

/// Public connection selectors only. Policy, credentials and file paths stay local.
pub(super) async fn connections(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    Json(json!({"connections":a.plugin_connections.iter().map(|(uid,c)|json!({"uid":uid,"plugin":c.identity,"targets":c.targets,"profiles":c.projects})).collect::<Vec<_>>()})).into_response()
}

/// Public plans retain their commitment but expose only this Run's target topology.
/// Submission uses the commitment and registered revision, never this presentation copy.
pub(super) fn public_plan(plan: &converyn_core::model::domain::Plan) -> serde_json::Value {
    let mut value = serde_json::to_value(plan).unwrap();
    if let Some(inventory) = &plan.input.inventory {
        let selected = plan
            .input
            .hosts
            .iter()
            .flat_map(|h| h.node_uids.iter().cloned())
            .collect();
        value["input"]["inventory"] =
            serde_json::to_value(inventory.restricted(&selected)).unwrap();
    }
    value
}
