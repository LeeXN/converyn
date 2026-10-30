//! Typed product administration and read models; writes still use the same
//! committed command path and its state-machine authorization.
use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditQuery {
    #[serde(default)]
    after: u64,
    #[serde(default = "page_limit")]
    limit: usize,
    #[serde(default = "audit_stream")]
    stream: String,
    #[serde(default = "audit_view")]
    view: String,
}
fn audit_view() -> String {
    "all".into()
}
fn page_limit() -> usize {
    50
}
fn audit_stream() -> String {
    "commands".into()
}
pub(super) async fn audit(
    State(a): State<App>,
    _headers: axum::http::HeaderMap,
    Query(q): Query<AuditQuery>,
) -> Response {
    if !matches!(q.view.as_str(), "all" | "activity" | "observations")
        || !(1..=100).contains(&q.limit)
        || ![
            "commands",
            "resolution",
            "identity",
            "security",
            "administration",
            "configuration",
            "history",
        ]
        .contains(&q.stream.as_str())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a
        .store
        .audit_view_page(q.stream.clone(), q.after, q.limit, q.view.clone())
        .await
    {
        Ok((rows, next)) => {
            Json(json!({"records":rows,"stream":q.stream,"view":q.view,"next_cursor":next}))
                .into_response()
        }
        Err(_) => unavailable(&a),
    }
}
pub(super) async fn me(State(a): State<App>) -> Response {
    let identity = REQUEST_AUTH.try_with(Clone::clone).ok();
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let personal = identity.as_ref().and_then(|auth| {
        m.state
            .personnel
            .caller(auth, crate::executors::host::now_ms())
    });
    Json(json!({"principal":identity.as_ref().map(|i|i.principal.as_str()).or(if a.auth.is_none(){Some("admin")}else{None}),
        "capabilities":{"platform":request_allows(&m.state,converyn_core::identities::access::Action::Platform,None,None),"access":request_allows(&m.state,converyn_core::identities::access::Action::Access,None,None),"super_admin":personal.is_some_and(|(i,_)|m.state.personnel.super_admin(&i.uid))},
        "identity":personal.map(|(i,_)| i),
        "credential":personal.map(|(_,c)|json!({"uid":c.uid,"fingerprint":c.fingerprint,"expires_ms":c.expires_ms,"overlap_until_ms":c.overlap_until_ms}))})).into_response()
}
pub(super) async fn metrics_prometheus(State(a): State<App>) -> Response {
    let v = a.raft.write_diagnostics();
    let mut out = format!(
        "converyn_controller_is_leader {}\n",
        usize::from(a.raft.metrics().borrow().current_leader == Some(a.id))
    );
    for field in [
        "in_flight",
        "ordinary_in_flight",
        "submitted",
        "committed",
        "conflicts",
        "queue_full",
        "outcome_pending",
        "failures",
        "last_success_ms",
        "last_failure_ms",
    ] {
        out.push_str(&format!("converyn_writes_{field} {}\n", v[field]));
    }
    if let Ok(m) = a.store.machine().await {
        out.push_str(&format!("converyn_runs_active {}\nconveryn_runs_needs_attention {}\nconveryn_audit_cold_records {}\n",m.state.runs.values().filter(|r|!r.status.terminal()).count(),m.state.runs.values().filter(|r|r.status==converyn_core::orchestration::machine::Status::NeedsAttention).count(),m.state.cold_audit.values().map(|h|h.count).sum::<u64>()));
    }
    out.push_str(&format!(
        "converyn_log_backend_available {}\n",
        usize::from(log_store(&a).is_some())
    ));
    if let Ok(m) = a.store.machine().await {
        let now = crate::executors::host::now_ms();
        let unknown_age = m
            .state
            .runs
            .values()
            .flat_map(|r| r.attempts.values())
            .filter(|v| v.status == converyn_core::orchestration::machine::Status::NeedsAttention)
            .filter_map(|v| v.authorized_ms)
            .map(|ms| now.saturating_sub(ms) / 1000)
            .max()
            .unwrap_or(0);
        let pending = m
            .state
            .runs
            .values()
            .flat_map(|r| r.attempts.iter().map(move |(id, v)| (&r.id, id, v)))
            .filter(|(_, _, v)| v.receipt.as_ref().is_some_and(|r| r.logs.is_some()))
            .filter(|(run, work, _)| {
                !m.state
                    .log_manifests
                    .contains_key(&format!("{run}/{work}/stdout"))
                    || !m
                        .state
                        .log_manifests
                        .contains_key(&format!("{run}/{work}/stderr"))
            })
            .count();
        let backlog = m.state.runs.len().saturating_sub(m.state.history.keep_runs);
        out.push_str(&format!("converyn_unknown_result_max_age_seconds {unknown_age}\nconveryn_log_upload_pending {pending}\nconveryn_archive_backlog {backlog}\nconveryn_log_pruned_streams {}\n",m.state.log_pruned.len()));
    }
    ([("content-type", "text/plain; version=0.0.4")], out).into_response()
}
