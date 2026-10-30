use super::*;
use base64::Engine;
use converyn_core::model::remote_logs::Manifest;
use serde::Deserialize;
use sha2::{Digest, Sha256};
const CHUNK: u64 = 65536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Chunk {
    #[serde(skip)]
    live: bool,
    #[serde(default)]
    grant: Option<String>,
    offset: u64,
    data: String,
    sha256: String,
    #[serde(default)]
    complete: bool,
    #[serde(default)]
    missing: bool,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct LogQuery {
    #[serde(default)]
    offset: u64,
}
fn prefix(cluster: &str, run: &str, work: &str, grant: &str, stream: &str) -> String {
    format!(
        "logs/{cluster}/{}/{stream}",
        converyn_core::model::domain::digest(&(run, work, grant))
    )
}
async fn readable(
    a: &App,
    _headers: &axum::http::HeaderMap,
    run: &str,
    work: &str,
    stream: &str,
) -> Result<
    (
        converyn_core::orchestration::machine::Attempt,
        Option<Manifest>,
    ),
    Response,
> {
    if !["stdout", "stderr"].contains(&stream) {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    if !confirm_read(a).await {
        return Err(unavailable(a));
    }
    let m = a.store.machine().await.map_err(|_| unavailable(a))?;
    if !request_view_run(&m.state, run) {
        return Err(denied("view"));
    }
    let r = a
        .store
        .load_run(run)
        .await
        .map_err(|_| unavailable(a))?
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
    if let Ok(auth) = REQUEST_AUTH.try_with(Clone::clone)
        && m.state
            .identities
            .bindings
            .get(&auth.principal)
            .is_some_and(|b| !b.fingerprints.contains(&auth.fingerprint))
    {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    if r.declared_work(work)
        .is_some_and(converyn_core::model::outputs::no_log)
    {
        return Err((StatusCode::CONFLICT,Json(json!({"code":"logs_suppressed","message":"sensitive output producer does not publish logs"}))).into_response());
    }
    let attempt = r
        .attempts
        .get(work)
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
    let manifest = m
        .state
        .log_manifests
        .get(&format!("{run}/{work}/{stream}"))
        .cloned();
    Ok(((**attempt).clone(), manifest))
}
pub(super) async fn upload_log(
    State(a): State<App>,
    ApiPath((node, run, work, stream)): ApiPath<(String, String, String, String)>,
    headers: axum::http::HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(chunk): Json<Chunk>,
) -> Response {
    let Ok(_permit) = a.log_admission.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "1")],
            Json(json!({"code":"log_upload_queue_full","retryable":true})),
        )
            .into_response();
    };
    let Ok(proof) = node_proof(&a, &node, &headers, &peer.0).await else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if !a
        .store
        .machine()
        .await
        .is_ok_and(|m| m.state.identity_current(&node, &proof, false))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(logs) = log_store(&a) else {
        return (
            if a.log_config.is_some(){StatusCode::SERVICE_UNAVAILABLE}else{StatusCode::NOT_IMPLEMENTED},
            Json(json!({"code":if a.log_config.is_some(){"log_storage_unavailable"}else{"log_storage_disabled"},"retryable":a.log_config.is_some()})),
        )
            .into_response();
    };
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(Some(r)) = a.store.load_run(&run).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !r
        .plan
        .work
        .iter()
        .any(|w| w.id == work && w.node_uid == node)
        || !["stdout", "stderr"].contains(&stream.as_str())
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if r.declared_work(&work)
        .is_some_and(converyn_core::model::outputs::no_log)
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"code":"logs_suppressed"})),
        )
            .into_response();
    }
    let attempt = &r.attempts[&work];
    let Some(grant) = &attempt.grant else {
        return StatusCode::CONFLICT.into_response();
    };
    if chunk.live && (chunk.grant.as_ref() != Some(grant) || attempt.receipt.is_some()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(data) = base64::engine::general_purpose::STANDARD.decode(&chunk.data) else {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    };
    if !chunk.offset.is_multiple_of(CHUNK)
        || data.len() > CHUNK as usize
        || chunk.offset + data.len() as u64 > converyn_core::model::logs::LOG_LIMIT
        || hex::encode(Sha256::digest(&data)) != chunk.sha256
        || (!chunk.live && !chunk.complete && data.len() != CHUNK as usize)
        || (chunk.live
            && (data.is_empty() || data.len() >= CHUNK as usize || chunk.complete || chunk.missing))
    {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    if chunk.missing && (!chunk.complete || !data.is_empty()) {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    if a.store.machine().await.is_ok_and(|m| {
        m.state
            .log_manifests
            .get(&format!("{run}/{work}/{stream}"))
            .is_some_and(|v| v.expires_ms <= crate::executors::host::now_ms())
    }) {
        return StatusCode::GONE.into_response();
    }
    let members = match a.store.machine().await {
        Ok(m) => m,
        Err(_) => return unavailable(&a),
    };
    if members
        .state
        .log_store_id
        .as_ref()
        .is_some_and(|id| id != &logs.id)
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"code":"log_store_identity_changed","retryable":false})),
        )
            .into_response();
    }
    if members.state.log_store_id.is_none() {
        let nodes = members
            .membership
            .membership()
            .nodes()
            .map(|(id, n)| (*id, n.clone()))
            .collect();
        if let Err(error) = verify_log_members(&a, &nodes).await {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"code":"log_storage_not_verified","message":error.to_string()})),
            )
                .into_response();
        }
        match a
            .raft
            .client_write(Command::Administer {
                change: converyn_core::orchestration::administration::Change::LogStorage {
                    store_id: logs.id.clone(),
                },
            })
            .await
        {
            Ok(r) if r.data.ok => {}
            _ => return unavailable(&a),
        }
    }
    let p = prefix(&a.cluster, &run, &work, grant, &stream);
    if chunk.live {
        let preview = json!({"offset":chunk.offset,"data":chunk.data,"sha256":chunk.sha256,"collected_ms":crate::executors::host::now_ms()});
        return match logs
            .put_live(&format!("{p}/live"), &serde_json::to_vec(&preview).unwrap())
            .await
        {
            Ok(()) => Json(json!({"ok":true,"next_offset":chunk.offset+data.len() as u64}))
                .into_response(),
            Err(_) => unavailable(&a),
        };
    }
    let result: Result<()> = async {
        if !data.is_empty() {
            logs.put(&format!("{p}/{:016x}", chunk.offset), &data)
                .await?;
        }
        if chunk.complete {
            let summary = attempt
                .receipt
                .as_ref()
                .and_then(|r| r.logs.as_ref())
                .context("terminal receipt not yet committed")?;
            let summary = if stream == "stdout" {
                &summary.stdout
            } else {
                &summary.stderr
            };
            let mut digest = Sha256::new();
            let mut bytes = 0;
            let mut chunk_hashes = Vec::new();
            while bytes
                < if chunk.missing {
                    chunk.offset
                } else {
                    summary.stored_bytes
                }
            {
                let b = logs
                    .get(&format!("{p}/{bytes:016x}"))
                    .await?
                    .context("log upload has a gap")?;
                ensure!(
                    !b.is_empty()
                        && b.len() <= CHUNK as usize
                        && bytes + b.len() as u64 <= summary.stored_bytes,
                    "invalid log chunk"
                );
                bytes += b.len() as u64;
                chunk_hashes.push(hex::encode(Sha256::digest(&b)));
                digest.update(b);
            }
            ensure!(
                chunk.missing || hex::encode(digest.finalize()) == summary.sha256,
                "terminal log digest mismatch"
            );
            let key = format!("{run}/{work}/{stream}");
            let m = a.store.machine().await?;
            let manifest = m
                .state
                .log_manifests
                .get(&key)
                .cloned()
                .unwrap_or(Manifest {
                    chunk_hashes,
                    missing_ranges: if chunk.missing {
                        vec![(chunk.offset, summary.stored_bytes)]
                    } else {
                        vec![]
                    },
                    run_id: run.clone(),
                    work_id: work.clone(),
                    node_uid: node.clone(),
                    grant: grant.clone(),
                    store_id: logs.id.clone(),
                    stream: stream.clone(),
                    bytes: summary.stored_bytes,
                    sha256: summary.sha256.clone(),
                    chunks: summary.stored_bytes.div_ceil(CHUNK),
                    truncated: summary.truncated,
                    expires_ms: crate::executors::host::now_ms()
                        + u64::from(logs.retention_days().max(30)) * 86400 * 1000,
                });
            let reply = a
                .raft
                .client_write(authorized(
                    Command::Administer {
                        change: converyn_core::orchestration::administration::Change::LogFinal {
                            manifest: manifest.clone(),
                        },
                    },
                    &a.peer_fingerprint,
                ))
                .await?;
            ensure!(
                reply.data.ok,
                "log manifest commit rejected: {}",
                reply.data.code
            );
            logs.put(&format!("{p}/manifest"), &serde_json::to_vec(&manifest)?)
                .await?;
            logs.delete(&format!("{p}/live")).await?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            Json(json!({"ok":true,"next_offset":chunk.offset+data.len() as u64})).into_response()
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"code":"log_upload_deferred","message":e.to_string(),"retryable":true})),
        )
            .into_response(),
    }
}
async fn log_chunk(
    a: &App,
    headers: &axum::http::HeaderMap,
    run: &str,
    work: &str,
    stream: &str,
    offset: u64,
) -> Result<serde_json::Value, Response> {
    let (attempt, manifest) = readable(a, headers, run, work, stream).await?;
    let logs = log_store(a).ok_or_else(|| (
        if a.log_config.is_some() { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::NOT_IMPLEMENTED },
        Json(json!({"code":if a.log_config.is_some(){"log_storage_unavailable"}else{"log_storage_disabled"},"retryable":a.log_config.is_some()})),
    ).into_response())?;
    if offset > converyn_core::model::logs::LOG_LIMIT {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    let grant = attempt
        .grant
        .as_deref()
        .ok_or_else(|| StatusCode::CONFLICT.into_response())?;
    if let Some(m) = &manifest {
        if m.store_id != logs.id {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"code":"log_store_identity_changed","retryable":false})),
            )
                .into_response());
        }
        if m.expires_ms <= crate::executors::host::now_ms() {
            return Ok(
                json!({"offset":offset,"next_offset":offset,"data":"","complete":true,"expired":true,"truncated":m.truncated}),
            );
        }
    }
    let p = prefix(&a.cluster, run, work, grant, stream);
    let base = offset / CHUNK * CHUNK;
    let mut data = logs
        .get(&format!("{p}/{base:016x}"))
        .await
        .map_err(|_| unavailable(a))?;
    if manifest.is_none() && data.as_ref().is_some_and(|b| b.len() < CHUNK as usize) {
        data = None;
    }
    let mut source = "stored_chunks";
    if manifest.is_none()
        && data.is_none()
        && let Some(b) = logs
            .get(&format!("{p}/live"))
            .await
            .map_err(|_| unavailable(a))?
    {
        let v: serde_json::Value = serde_json::from_slice(&b).map_err(|_| unavailable(a))?;
        if v["offset"].as_u64() == Some(base)
            && crate::executors::host::now_ms()
                .saturating_sub(v["collected_ms"].as_u64().unwrap_or(0))
                < 60_000
        {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(v["data"].as_str().unwrap_or_default())
                .map_err(|_| unavailable(a))?;
            if hex::encode(Sha256::digest(&bytes)) != v["sha256"].as_str().unwrap_or_default() {
                return Err(unavailable(a));
            }
            data = Some(bytes);
            source = "live_fragment";
        }
    }

    if manifest.as_ref().is_some_and(|m| {
        m.missing_ranges
            .iter()
            .any(|(a, b)| offset >= *a && offset < *b)
    }) {
        return Ok(
            json!({"offset":offset,"next_offset":offset,"data":"","complete":true,"expired":false,"truncated":true,"condition":"upload_incomplete","missing_ranges":manifest.as_ref().map(|m|&m.missing_ranges)}),
        );
    }
    if let (Some(m), Some(bytes)) = (&manifest, &data)
        && (bytes.len() > CHUNK as usize
            || base + bytes.len() as u64 > m.bytes
            || m.chunk_hashes.get((offset / CHUNK) as usize)
                != Some(&hex::encode(Sha256::digest(bytes))))
    {
        return Err((StatusCode::SERVICE_UNAVAILABLE,Json(json!({"code":"log_chunk_checksum_mismatch","message":"Stored log bytes differ from the committed manifest; verify the storage and restore matching evidence","retryable":false}))).into_response());
    }
    let data = data.map(|b| {
        b.get((offset - base) as usize..)
            .unwrap_or_default()
            .to_vec()
    });
    let next = offset + data.as_ref().map_or(0, |b| b.len() as u64);
    if data.is_none() && manifest.as_ref().is_some_and(|m| offset < m.bytes) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"code":"log_chunk_missing","retryable":true})),
        )
            .into_response());
    }
    Ok(
        json!({"source":source,"condition":if manifest.is_some(){"stored"}else if attempt.receipt.is_some(){"upload_pending"}else{"running"},"offset":offset,"next_offset":next,"data":base64::engine::general_purpose::STANDARD.encode(data.unwrap_or_default()),"complete":manifest.as_ref().is_some_and(|m|next>=m.bytes),"sha256":manifest.as_ref().map(|m|&m.sha256),"verified":manifest.is_some(),"expired":false,"truncated":manifest.as_ref().is_some_and(|m|m.truncated)}),
    )
}
pub(super) async fn read_log(
    State(a): State<App>,
    ApiPath((run, work, stream)): ApiPath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    Query(q): Query<LogQuery>,
) -> Response {
    match log_chunk(&a, &headers, &run, &work, &stream, q.offset).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => r,
    }
}
/// SSE errors bypass the HTTP error middleware after headers are sent. Preserve
/// the original code and retry policy instead of reducing every error to "503".
async fn log_diagnostic(response: Response, correlation: Option<&str>) -> serde_json::Value {
    let status = response.status();
    let raw = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap_or_default();
    let mut value = serde_json::from_slice::<serde_json::Value>(&raw)
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({}));
    value["status"] = status.as_u16().into();
    if value.get("code").is_none() {
        value["code"] = match status.as_u16() {
            401 => "unauthorized",
            403 => "forbidden",
            503 => "temporarily_unavailable",
            _ => "log_read_failed",
        }
        .into();
    }
    if value.get("message").is_none() {
        value["message"] = status
            .canonical_reason()
            .unwrap_or("Log read failed")
            .into();
    }
    if value.get("retryable").is_none() {
        value["retryable"] = matches!(status.as_u16(), 408 | 429 | 502 | 503 | 504).into();
    }
    value["request_id"] = correlation.unwrap_or_default().into();
    value
}

pub(super) async fn log_events(
    State(a): State<App>,
    ApiPath((run, work, stream)): ApiPath<(String, String, String)>,
    headers: axum::http::HeaderMap,
    Query(q): Query<LogQuery>,
) -> Response {
    let first = match log_chunk(&a, &headers, &run, &work, &stream, q.offset).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let auth = REQUEST_AUTH.try_with(Clone::clone).ok();
    let correlation = request_id();
    let events = futures_util::stream::unfold(
        (
            a,
            headers,
            run,
            work,
            stream,
            Some(first),
            q.offset,
            false,
            auth,
            correlation,
        ),
        |(a, headers, run, work, stream, mut first, offset, done, auth, correlation)| async move {
            if done {
                return None;
            }
            let result = if let Some(v) = first.take() {
                Ok(v)
            } else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let call = log_chunk(&a, &headers, &run, &work, &stream, offset);
                if let Some(auth) = auth.clone() {
                    REQUEST_AUTH.scope(auth, call).await
                } else {
                    call.await
                }
            };
            let (event, next, done) = match result {
                Ok(v) => {
                    let next = v["next_offset"].as_u64().unwrap_or(offset);
                    let done = v["complete"] == true;
                    (
                        axum::response::sse::Event::default()
                            .event("log")
                            .json_data(v)
                            .unwrap(),
                        next,
                        done,
                    )
                }
                Err(r) => (
                    axum::response::sse::Event::default()
                        .event("error")
                        .json_data(log_diagnostic(r, correlation.as_deref()).await)
                        .unwrap(),
                    offset,
                    true,
                ),
            };
            Some((
                Ok::<_, std::convert::Infallible>(event),
                (
                    a,
                    headers,
                    run,
                    work,
                    stream,
                    first,
                    next,
                    done,
                    auth,
                    correlation,
                ),
            ))
        },
    );
    axum::response::Sse::new(events)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}
pub(super) async fn download_log(
    State(a): State<App>,
    ApiPath((run, work, stream)): ApiPath<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> Response {
    let (attempt, manifest) = match readable(&a, &headers, &run, &work, &stream).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(manifest) = manifest else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"code":"log_upload_incomplete"})),
        )
            .into_response();
    };
    if !manifest.missing_ranges.is_empty() {
        return (
            StatusCode::CONFLICT,
            Json(json!({"code":"log_upload_incomplete","missing_ranges":manifest.missing_ranges})),
        )
            .into_response();
    }
    if manifest.expires_ms <= crate::executors::host::now_ms() {
        return StatusCode::GONE.into_response();
    }
    let Some(logs) = log_store(&a) else {
        return (
            if a.log_config.is_some() { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::NOT_IMPLEMENTED },
            Json(json!({"code":if a.log_config.is_some(){"log_storage_unavailable"}else{"log_storage_disabled"},"retryable":a.log_config.is_some()})),
        ).into_response();
    };
    if manifest.store_id != logs.id {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"code":"log_store_identity_changed","retryable":false})),
        )
            .into_response();
    }
    let p = prefix(
        &a.cluster,
        &run,
        &work,
        attempt.grant.as_deref().unwrap_or_default(),
        &stream,
    );
    let authorization = REQUEST_AUTH.try_with(Clone::clone).ok();
    let run_id = run.clone();
    let expected = manifest.sha256.clone();
    let hashes = manifest.chunk_hashes.clone();
    let total = manifest.bytes;
    let body = futures_util::stream::try_unfold(
        (
            a,
            logs,
            p,
            0u64,
            total,
            Sha256::new(),
            expected,
            hashes,
            authorization,
            run_id,
        ),
        |(a, logs, p, offset, total, mut digest, expected, hashes, authorization, run_id)| async move {
            if offset >= total {
                return Ok::<_, std::io::Error>(None);
            }
            let permitted = if let Some(auth) = authorization.clone() {
                REQUEST_AUTH
                    .scope(auth, async {
                        if !confirm_read(&a).await {
                            return false;
                        }
                        a.store
                            .machine()
                            .await
                            .is_ok_and(|m| request_view_run(&m.state, &run_id))
                    })
                    .await
            } else {
                confirm_read(&a).await
            };
            if !permitted {
                return Err(std::io::Error::other(
                    "log download authorization or quorum changed",
                ));
            }
            let bytes = logs
                .get(&format!("{p}/{offset:016x}"))
                .await
                .map_err(std::io::Error::other)?
                .ok_or_else(|| std::io::Error::other("missing log chunk"))?;
            let next = offset + bytes.len() as u64;
            if bytes.is_empty()
                || bytes.len() > CHUNK as usize
                || next > total
                || hashes.get((offset / CHUNK) as usize)
                    != Some(&hex::encode(Sha256::digest(&bytes)))
            {
                return Err(std::io::Error::other("log chunk checksum mismatch"));
            }
            digest.update(&bytes);
            if next == total && hex::encode(digest.clone().finalize()) != expected {
                return Err(std::io::Error::other("log download checksum mismatch"));
            }
            Ok(Some((
                bytes,
                (
                    a,
                    logs,
                    p,
                    next,
                    total,
                    digest,
                    expected,
                    hashes,
                    authorization,
                    run_id,
                ),
            )))
        },
    );
    let mut response = (
        [
            ("content-type", "application/octet-stream"),
            ("content-disposition", "attachment; filename=task.log"),
        ],
        axum::body::Body::from_stream(body),
    )
        .into_response();
    response.headers_mut().insert(
        "content-length",
        HeaderValue::from_str(&total.to_string()).unwrap(),
    );
    response.headers_mut().insert(
        "x-converyn-sha256",
        HeaderValue::from_str(&manifest.sha256).unwrap(),
    );
    response
}
pub(super) async fn log_retention(a: App) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        if a.raft.metrics().borrow().current_leader != Some(a.id) {
            continue;
        }
        let Some(logs) = log_store(&a) else { continue };
        let Ok(m) = a.store.machine().await else {
            continue;
        };
        for manifest in m.state.log_manifests.values().filter(|v| {
            (v.expires_ms <= crate::executors::host::now_ms()
                || m.state
                    .retirement_proofs
                    .contains_key(&format!("run/{}", v.run_id)))
                && v.store_id == logs.id
                && !m.state.log_pruned.contains_key(&v.key())
        }) {
            let p = prefix(
                &a.cluster,
                &manifest.run_id,
                &manifest.work_id,
                &manifest.grant,
                &manifest.stream,
            );
            let mut complete = logs.delete(&format!("{p}/live")).await.is_ok();
            for i in 0..manifest.chunks {
                if let Err(error) = logs.delete(&format!("{p}/{:016x}", i * CHUNK)).await {
                    tracing::warn!(%error,"expired log cleanup deferred");
                    complete = false;
                    break;
                }
            }
            if complete {
                let _ = a
                    .raft
                    .client_write(Command::Administer {
                        change: converyn_core::orchestration::administration::Change::LogPruned {
                            key: manifest.key(),
                            store_id: manifest.store_id.clone(),
                            sha256: manifest.sha256.clone(),
                        },
                    })
                    .await;
            }
        }
    }
}

pub(super) async fn verify_log_members(
    a: &App,
    nodes: &std::collections::BTreeMap<u64, openraft::BasicNode>,
) -> Result<()> {
    let Some(logs) = log_store(a) else {
        ensure!(a.log_config.is_none(), "log backend is not ready");
        return Ok(());
    };
    if nodes.len() > 1 {
        ensure!(
            logs.shared(),
            "multiple controllers require S3 or a shared log directory"
        )
    }
    let mut verified = 1;
    for (id, node) in nodes {
        if *id == a.id {
            continue;
        }
        let response = a
            .peer_http
            .get(format!(
                "{}://{}/health/live",
                if a.https { "https" } else { "http" },
                node.addr
            ))
            .timeout(Duration::from_secs(3))
            .send()
            .await;
        let Ok(response) = response else { continue };
        let response = response.error_for_status()?;
        let v: serde_json::Value = response.json().await?;
        ensure!(
            v["node_id"] == *id
                && v["control_plane"] == a.cluster
                && v["log_storage"]["id"] == logs.id
                && v["log_storage"]["shared"] == true,
            "peer log backend identity or sharing differs"
        );
        let probe = v["log_storage"]["probe"]
            .as_str()
            .context("peer storage probe missing")?;
        ensure!(
            converyn_core::identities::registration::valid_sha(probe),
            "invalid storage probe"
        );
        ensure!(
            logs.get(&format!("probes/{}/{id}/{probe}", a.cluster))
                .await?
                .as_deref()
                == Some(probe.as_bytes()),
            "peer log path is not the same physical storage"
        );
        verified += 1;
    }
    ensure!(
        verified > nodes.len() / 2,
        "cannot verify log storage with a quorum"
    );
    Ok(())
}
pub(super) async fn log_storage_status(State(a): State<App>) -> Response {
    let logs = log_store(&a);
    Json(json!({"enabled":a.log_config.is_some(),"available":logs.is_some(),"store_id":logs.as_ref().map(|s|&s.id)})).into_response()
}

pub(super) fn log_store(a: &App) -> Option<crate::log_store::Store> {
    a.logs.read().ok().and_then(|s| s.clone())
}
pub(super) async fn log_backend(a: App) {
    let Some(config) = &a.log_config else { return };
    let mut delay = 2;
    loop {
        if log_store(&a).is_none() {
            let result: Result<crate::log_store::Store> = async {
                let store = crate::log_store::Store::open(config.clone()).await?;
                store
                    .put(
                        &format!("probes/{}/{}/{}", a.cluster, a.id, a.log_probe),
                        a.log_probe.as_bytes(),
                    )
                    .await?;
                Ok(store)
            }
            .await;
            match result {
                Ok(store) => {
                    if let Ok(mut cached) = a.logs.write() {
                        *cached = Some(store);
                    }
                    delay = 2;
                }
                Err(error) => {
                    tracing::warn!(%error,"log backend unavailable; control plane remains online");
                    delay = (delay * 2).min(30);
                }
            }
        }
        if let Some(store) = log_store(&a) {
            tokio::time::sleep(Duration::from_secs(30)).await;
            if !store.verify_identity().await.unwrap_or(false) {
                if let Ok(mut cache) = a.logs.write() {
                    *cache = None;
                }
                tracing::warn!(
                    "log backend health check failed; raw-log transfer will retry independently"
                );
            }
        } else {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    }
}

/// Partial output has its own endpoint and mutable namespace; no receipt is changed.
pub(super) async fn upload_live(
    state: State<App>,
    path: ApiPath<(String, String, String, String)>,
    headers: axum::http::HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(mut chunk): Json<Chunk>,
) -> Response {
    chunk.live = true;
    upload_log(state, path, headers, peer, Json(chunk)).await
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[tokio::test]
    async fn sse_errors_preserve_semantics_and_request_correlation() {
        let response = (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"code":"log_chunk_checksum_mismatch","message":"corrupt log","retryable":false}))).into_response();
        let value = log_diagnostic(response, Some("request-123")).await;
        assert_eq!(value["status"], 503);
        assert_eq!(value["code"], "log_chunk_checksum_mismatch");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["request_id"], "request-123");
        let value = log_diagnostic(
            StatusCode::SERVICE_UNAVAILABLE.into_response(),
            Some("request-456"),
        )
        .await;
        assert_eq!(value["retryable"], true);
        assert_eq!(value["code"], "temporarily_unavailable");
        let value = log_diagnostic(StatusCode::FORBIDDEN.into_response(), None).await;
        assert_eq!(value["retryable"], false);
    }
}
