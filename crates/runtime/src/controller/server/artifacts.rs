//! Managed artifact uploads, peer durability and assignment-scoped binary downloads.
use super::*;
use crate::artifacts::managed;
use axum::http::{HeaderMap, header};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactUpload {
    size: u64,
    #[serde(default)]
    offset: u64,
}

fn artifact_error(error: impl std::fmt::Display) -> Response {
    (StatusCode::CONFLICT, Json(json!({"error":"artifact_transfer_rejected","detail":error.to_string(),"retryable":true}))).into_response()
}
fn peer_headers(a: &App) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let token = a.auth.as_ref().map(|v| v.peer.as_str()).unwrap_or(&a.token);
    let mut auth = HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
    auth.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, auth);
    headers.insert(
        "x-converyn-cluster",
        HeaderValue::from_str(&a.cluster).unwrap(),
    );
    headers.insert(
        "x-converyn-protocol",
        HeaderValue::from_str(&crate::controller::network::PROTOCOL_VERSION.to_string()).unwrap(),
    );
    headers.insert(
        "x-converyn-plugin-manifest",
        HeaderValue::from_str(&crate::plugins::registry().fingerprint()).unwrap(),
    );
    headers
}
fn peers(a: &App) -> Vec<(u64, openraft::BasicNode)> {
    let metrics = a.raft.metrics().borrow().clone();
    let membership = metrics.membership_config.membership();
    membership
        .voter_ids()
        .filter(|id| *id != a.id)
        .filter_map(|id| membership.get_node(&id).cloned().map(|n| (id, n)))
        .collect()
}
async fn verify_peer(a: &App, id: u64, node: &openraft::BasicNode) -> Result<()> {
    Network {
        https: a.https,
        token: a
            .auth
            .as_ref()
            .map(|v| v.peer.clone())
            .unwrap_or_else(|| a.token.clone()),
        cluster: a.cluster.clone(),
        client: a.peer_http.clone(),
        store: Some(a.store.clone()),
        peer_fingerprints: a
            .auth
            .as_ref()
            .and_then(|v| v.certificates.get("peer"))
            .cloned()
            .unwrap_or_default(),
    }
    .verify_target(id, node, Duration::from_secs(3))
    .await
}
async fn replicate(
    a: &App,
    sha: &str,
    query: &ArtifactUpload,
    bytes: Option<axum::body::Bytes>,
) -> Result<()> {
    for (id, node) in peers(a) {
        verify_peer(a, id, &node).await?;
        let path = if bytes.is_some() {
            format!(
                "/internal/raft/artifacts/{sha}/chunks?size={}&offset={}",
                query.size, query.offset
            )
        } else {
            format!(
                "/internal/raft/artifacts/{sha}/complete?size={}",
                query.size
            )
        };
        let request = a
            .peer_http
            .post(format!(
                "{}://{}{path}",
                if a.https { "https" } else { "http" },
                node.addr
            ))
            .headers(peer_headers(a))
            .timeout(Duration::from_secs(3600));
        let response = if let Some(bytes) = bytes.clone() {
            request.body(bytes)
        } else {
            request
        }
        .send()
        .await?;
        ensure!(
            response.status().is_success(),
            "artifact not durable on controller {}; retry upload",
            node.addr
        );
    }
    Ok(())
}
async fn write_chunk(
    a: &App,
    sha: &str,
    q: &ArtifactUpload,
    bytes: axum::body::Bytes,
) -> Result<()> {
    let root = a.artifact_root.clone();
    let sha = sha.to_owned();
    let size = q.size;
    let offset = q.offset;
    tokio::task::spawn_blocking(move || managed::put_chunk(&root, &sha, size, offset, &bytes))
        .await??;
    Ok(())
}
async fn finish_blob(a: &App, sha: &str, size: u64) -> Result<()> {
    let root = a.artifact_root.clone();
    let sha = sha.to_owned();
    tokio::task::spawn_blocking(move || managed::finish(&root, &sha, size)).await??;
    Ok(())
}
pub(super) async fn upload_artifact_chunk(
    State(a): State<App>,
    ApiPath(sha): ApiPath<String>,
    Query(q): Query<ArtifactUpload>,
    bytes: axum::body::Bytes,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(_permit) = a.artifact_admission.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let result = async {
        write_chunk(&a, &sha, &q, bytes.clone()).await?;
        replicate(&a, &sha, &q, Some(bytes.clone())).await
    }
    .await;
    match result {
        Ok(()) => {
            Json(json!({"ok":true,"next_offset":q.offset+bytes.len() as u64})).into_response()
        }
        Err(e) => artifact_error(e),
    }
}
pub(super) async fn peer_artifact_chunk(
    State(a): State<App>,
    ApiPath(sha): ApiPath<String>,
    Query(q): Query<ArtifactUpload>,
    bytes: axum::body::Bytes,
) -> Response {
    match write_chunk(&a, &sha, &q, bytes).await {
        Ok(()) => Json(json!({"ok":true})).into_response(),
        Err(e) => artifact_error(e),
    }
}
pub(super) async fn peer_artifact_complete(
    State(a): State<App>,
    ApiPath(sha): ApiPath<String>,
    Query(q): Query<ArtifactUpload>,
) -> Response {
    match finish_blob(&a, &sha, q.size).await {
        Ok(()) => Json(json!({"ok":true})).into_response(),
        Err(e) => artifact_error(e),
    }
}
pub(super) async fn complete_artifact(
    State(a): State<App>,
    ApiPath(sha): ApiPath<String>,
    Query(q): Query<ArtifactUpload>,
) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(_permit) = a.artifact_admission.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let membership = a.raft.metrics().borrow().membership_config.clone();
    if let Err(e) = async {
        finish_blob(&a, &sha, q.size).await?;
        replicate(&a, &sha, &q, None).await
    }
    .await
    {
        return artifact_error(e);
    }
    if a.raft.metrics().borrow().membership_config != membership {
        return artifact_error("membership changed; retry the upload");
    }
    command(
        State(a),
        Json(Command::Administer {
            change: converyn_core::orchestration::administration::Change::Artifact {
                sha256: sha,
                size: q.size,
            },
        }),
    )
    .await
}
pub(super) async fn artifact_catalog(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    Json(json!({"artifacts":m.state.administration.artifacts.iter().map(|(sha,size)|json!({"sha256":sha,"size":size})).collect::<Vec<_>>()})).into_response()
}
// A controller added after the upload can recover blobs from a retained peer.
async fn ensure_blob(a: &App, sha: &str, size: u64) -> Result<std::path::PathBuf> {
    let _recovery = a.artifact_recovery.lock().await;
    if let Ok(path) = managed::path(&a.artifact_root, sha, size) {
        return Ok(path);
    }
    let _reservation = managed::recovery_reservation(&a.artifact_root, sha, size)?;
    for (id, node) in peers(a) {
        if verify_peer(a, id, &node).await.is_err() {
            continue;
        }
        let url = format!(
            "{}://{}/internal/raft/artifacts/{sha}?size={size}",
            if a.https { "https" } else { "http" },
            node.addr
        );
        let fetched = crate::artifacts::artifact::fetch_with_headers(
            &a.peer_http,
            &url,
            crate::artifacts::artifact::Artifact {
                sha256: sha.into(),
                size,
            },
            &a.artifact_root,
            2,
            None,
            &peer_headers(a),
        )
        .await;
        if let Ok(download) = fetched {
            return Ok(download.path);
        }
    }
    anyhow::bail!("managed artifact bytes unavailable; re-register the retained local file")
}
async fn binary_response(
    path: std::path::PathBuf,
    sha: &str,
    size: u64,
    headers: &HeaderMap,
) -> Result<Response> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut offset = 0;
    if let Some(range) = headers.get(header::RANGE) {
        let range = range
            .to_str()?
            .strip_prefix("bytes=")
            .context("invalid artifact range")?;
        offset = range
            .strip_suffix('-')
            .context("only open byte ranges are supported")?
            .parse()?;
        ensure!(offset < size, "artifact range outside object");
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut file = tokio::fs::File::from_std(file);
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let stream = futures_util::stream::try_unfold(
        (file, size - offset),
        |(mut file, remaining)| async move {
            if remaining == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let mut bytes = vec![0; remaining.min(managed::CHUNK as u64) as usize];
            file.read_exact(&mut bytes).await?;
            let next = remaining - bytes.len() as u64;
            Ok(Some((bytes, (file, next))))
        },
    );
    let mut response = axum::body::Body::from_stream(stream).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&(size - offset).to_string())?,
    );
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response
        .headers_mut()
        .insert(header::ETAG, HeaderValue::from_str(&format!("\"{sha}\""))?);
    if offset > 0 {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {offset}-{}/{size}", size - 1))?,
        );
    }
    Ok(response)
}
pub(super) async fn peer_artifact_read(
    State(a): State<App>,
    ApiPath(sha): ApiPath<String>,
    Query(q): Query<ArtifactUpload>,
    headers: HeaderMap,
) -> Response {
    // Do not recursively fetch from peers here: a missing replica is reported as missing.
    let Ok(path) = managed::path(&a.artifact_root, &sha, q.size) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match binary_response(path, &sha, q.size, &headers).await {
        Ok(r) => r,
        Err(e) => artifact_error(e),
    }
}
pub(super) async fn work_artifact(
    State(a): State<App>,
    ApiPath((node, run_id, work_id, sha)): ApiPath<(String, String, String, String)>,
    headers: HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
) -> Response {
    if let Err(r) = node_proof(&a, &node, &headers, &peer.0).await {
        return r;
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(Some(run)) = a.store.load_run(&run_id).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(work) = run.effective_work(&work_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(artifact) = work.operation.artifact() else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if work.node_uid != node
        || !artifact.locations.is_empty()
        || artifact.sha256 != sha
        || run.status.terminal()
        || run.cancel_requested
        || run.pause_requested
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    if m.state.administration.artifacts.get(&sha) != Some(&artifact.size) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match ensure_blob(&a, &sha, artifact.size).await {
        Ok(path) => match binary_response(path, &sha, artifact.size, &headers).await {
            Ok(r) => r,
            Err(e) => artifact_error(e),
        },
        Err(e) => artifact_error(e),
    }
}
