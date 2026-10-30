//! 通用目标、资源所有权、历史与备份管理。
use super::*;

pub(super) async fn certificate_binding(
    State(a): State<App>,
    Json(change): Json<converyn_core::identities::identity::Change>,
) -> Response {
    if !change.valid() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"invalid_certificate_binding"})),
        )
            .into_response();
    }
    command(State(a), Json(Command::SetCertificateBinding { change })).await
}

pub(super) async fn certificate_bindings(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    match a.store.machine().await {
        Ok(m) => Json(json!({"identities":m.state.identities})).into_response(),
        Err(_) => unavailable(&a),
    }
}

pub(super) async fn node_group_command(
    State(a): State<App>,
    Json(change): Json<converyn_core::orchestration::administration::Change>,
) -> Response {
    if matches!(
        &change,
        converyn_core::orchestration::administration::Change::Secret { .. }
            | converyn_core::orchestration::administration::Change::LogStorage { .. }
            | converyn_core::orchestration::administration::Change::Artifact { .. }
            | converyn_core::orchestration::administration::Change::LogFinal { .. }
            | converyn_core::orchestration::administration::Change::LogPruned { .. }
    ) {
        return (StatusCode::FORBIDDEN,Json(json!({"code":"internal_administration_action","message":"Use the dedicated identity or log protocol"}))).into_response();
    }
    command(State(a), Json(Command::Administer { change })).await
}

pub(super) async fn node_groups(State(a): State<App>, Query(q): Query<PageQuery>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let context = format!("node-groups/{}", access_cursor_context(&m.state));
    let after = match q.after(&context) {
        Ok(after) => after,
        Err(response) => return response,
    };
    let mut rows = m
        .state
        .node_groups
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .take(q.limit + 1)
        .collect::<Vec<_>>();
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let cursor = if more {
        rows.last().map(|(id, _)| q.cursor(&context, id))
    } else {
        None
    };
    let admin = &m.state.administration;
    Json(json!({"node_groups":rows.into_iter().collect::<BTreeMap<_,_>>(),"administration":{"maintenance":admin.maintenance,"owner_revisions":admin.owner_revisions.iter().take(q.limit).collect::<BTreeMap<_,_>>(),"windows":admin.windows.iter().take(q.limit).collect::<BTreeMap<_,_>>(),"retired_node_groups":admin.retired_node_groups.iter().take(q.limit).collect::<BTreeMap<_,_>>(),"audit":admin.audit},"resource_owners":m.state.resource_owners.iter().take(q.limit).collect::<BTreeMap<_,_>>(),"next_cursor":cursor})).into_response()
}

pub(super) fn history_candidate(
    state: &converyn_core::orchestration::machine::State,
) -> Option<converyn_core::orchestration::history::Change> {
    use converyn_core::orchestration::history::{Change, Kind};
    let candidate = if state.runs.len() > state.history.keep_runs {
        state
            .runs
            .values()
            .filter(|r| !state.run_protected(&r.id))
            .min_by_key(|r| r.sequence)
            .map(|r| (Kind::Run, r.id.clone(), r.revision))
    } else {
        None
    };
    let candidate = candidate.or_else(|| {
        if state.scripts.values().map(|s| s.len() as u64).sum::<u64>()
            > state.history.definition_hot_bytes
        {
            state
                .scripts
                .keys()
                .next()
                .map(|uid| (Kind::Definition, uid.clone(), 0))
        } else {
            None
        }
    });
    candidate.map(|(kind, uid, expected_revision)| Change::Archive {
        kind,
        uid,
        expected_revision,
        now_ms: crate::executors::host::now_ms(),
    })
}

pub(super) async fn history_catalog(State(a): State<App>, Query(q): Query<PageQuery>) -> Response {
    let after = match q.after("history") {
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
        .history
        .records
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .filter(|(_, r)| status_matches(r.status, &q.status))
        .take(q.limit + 1)
        .collect::<Vec<_>>();
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let cursor = if more {
        rows.last().map(|(id, _)| q.cursor("history", id))
    } else {
        None
    };
    let h = &m.state.history;
    Json(json!({"records":rows.into_iter().collect::<BTreeMap<_,_>>(),"keep_runs":h.keep_runs,"definition_hot_bytes":h.definition_hot_bytes,"configuration_revision":h.configuration_revision,"run_sequence":m.state.run_sequence,"audit":h.audit,"next_cursor":cursor})).into_response()
}

pub(super) async fn history_command(
    State(a): State<App>,
    Json(mut change): Json<converyn_core::orchestration::history::Change>,
) -> Response {
    if let converyn_core::orchestration::history::Change::Archive { now_ms, .. } = &mut change {
        *now_ms = crate::executors::host::now_ms();
    }
    command(State(a), Json(Command::History { change })).await
}

pub(super) async fn backup(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok((path, _)) = a.store.backup_carrier().await else {
        return unavailable(&a);
    };
    let Ok(file) = tokio::fs::File::open(&path).await else {
        let _ = std::fs::remove_file(path);
        return unavailable(&a);
    };
    struct StreamFile {
        file: tokio::fs::File,
        path: std::path::PathBuf,
    }
    impl Drop for StreamFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    let stream =
        futures_util::stream::try_unfold(StreamFile { file, path }, |mut source| async move {
            use tokio::io::AsyncReadExt;
            let mut bytes = vec![0; 256 * 1024];
            let n = source.file.read(&mut bytes).await?;
            if n == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            bytes.truncate(n);
            Ok(Some((bytes, source)))
        });
    (
        [("content-type", "application/vnd.converyn.backup.v2")],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

pub(super) async fn plugin_catalog() -> Response {
    Json(json!({"plugins":crate::plugins::registry().descriptors(),"manifest_sha256":crate::plugins::registry().fingerprint()})).into_response()
}

pub(super) async fn targets(
    State(a): State<App>,
    _headers: axum::http::HeaderMap,
    Query(q): Query<PageQuery>,
) -> Response {
    let after = match q.after("targets") {
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
        .targets
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .take(q.limit + 1)
        .collect::<Vec<_>>();
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let cursor = if more {
        rows.last().map(|(id, _)| q.cursor("targets", id))
    } else {
        None
    };
    Json(json!({"targets":rows.into_iter().collect::<BTreeMap<_,_>>(),"next_cursor":cursor}))
        .into_response()
}
