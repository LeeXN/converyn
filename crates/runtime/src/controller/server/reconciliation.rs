//! 内部期望状态维护、持续核对和只读诊断。
use super::*;

pub(super) async fn deployment(
    State(a): State<App>,
    ApiPath(id): ApiPath<String>,
    _headers: axum::http::HeaderMap,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
        Ok(m) => match m.state.deployments.get(&id) {
            Some(d) => Json(d).into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        Err(_) => unavailable(&a),
    }
}

pub(super) async fn reconcile_loop(a: App) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if a.raft.metrics().borrow().current_leader != Some(a.id) {
            continue;
        }
        let Ok(m) = a.store.machine().await else {
            continue;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_millis() as u64);
        if m.state
            .registry
            .bootstraps
            .values()
            .any(|g| g.expires_ms <= now)
        {
            let _ = a
                .raft
                .client_write(Command::Identity {
                    change: converyn_core::identities::registration::Change::ExpireBootstrap {
                        now_ms: now,
                    },
                })
                .await;
        }
        for run in m.state.runs.values().filter(|r| !r.status.terminal()) {
            if run.preparing
                && !run.pause_requested
                && !run
                    .plan
                    .lock_keys()
                    .iter()
                    .any(|key| m.state.execution_locks.contains_key(key))
                && run
                    .plan
                    .work
                    .iter()
                    .all(|w| w.operation.artifact().is_none() || run.prepared.contains(&w.id))
            {
                let _ = command(
                    State(a.clone()),
                    Json(Command::Preparation {
                        change: converyn_core::orchestration::preparation::Change::Admit {
                            run_id: run.id.clone(),
                            expected_revision: run.revision,
                        },
                    }),
                )
                .await;
            }
            for work in &run.plan.work {
                if let converyn_core::model::domain::Operation::Reboot {
                    recovery_timeout_seconds,
                    ..
                } = &work.operation
                {
                    let attempt = &run.attempts[&work.id];
                    if matches!(
                        attempt.status,
                        converyn_core::orchestration::machine::Status::Authorized
                            | converyn_core::orchestration::machine::Status::Running
                            | converyn_core::orchestration::machine::Status::CancelRequested
                    ) && attempt.authorized_ms.is_some_and(|t| {
                        now.saturating_sub(t) > u64::from(*recovery_timeout_seconds) * 1000
                    }) {
                        let _ = tokio::time::timeout(
                            Duration::from_secs(3),
                            a.raft.client_write(Command::MarkUncertain {
                                run_id: run.id.clone(),
                                work_id: work.id.clone(),
                                expected_revision: run.revision,
                                reason: Some("reboot_recovery_deadline_exceeded".into()),
                            }),
                        )
                        .await;
                    }
                }
            }
        }
        for uid in m.state.drift.keys() {
            if m.state.drift_due(uid, now)
                && let Some(d) = m.state.deployments.get(uid)
            {
                let _ = tokio::time::timeout(
                    Duration::from_secs(3),
                    a.raft.client_write(Command::ReconcileDrift {
                        uid: uid.clone(),
                        now_ms: now,
                        expected_generation: d.generation,
                    }),
                )
                .await;
            }
        }
        for d in m.state.deployments.values() {
            let active_changed = d
                .active_run
                .as_ref()
                .and_then(|id| m.state.runs.get(id))
                .is_some_and(|r| {
                    r.status.terminal()
                        || (r.status
                            == converyn_core::orchestration::machine::Status::NeedsAttention
                            && d.condition != "needs_attention")
                });
            let pending = d.admission_pending()
                && !d
                    .plan
                    .lock_keys()
                    .iter()
                    .any(|key| m.state.execution_locks.contains_key(key));
            if active_changed || pending {
                let _ = tokio::time::timeout(
                    Duration::from_secs(3),
                    a.raft
                        .client_write(Command::ReconcileDeployment { uid: d.uid.clone() }),
                )
                .await;
            }
        }
    }
}

pub(super) async fn drift_status(
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
    if !m.state.deployments.contains_key(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(json!({"policy":m.state.drift.get(&id)})).into_response()
}
