//! 一次性登记、续期和吊销，不复制 Node 私钥。
use super::*;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BootstrapRequest {
    id: String,
    node_uid: String,
    token_sha256: String,
    csr_pem: String,
    ttl_seconds: u64,
    #[serde(default)]
    recover: bool,
}

pub(super) async fn identity_write(
    a: &App,
    change: converyn_core::identities::registration::Change,
) -> Result<(), Response> {
    match a
        .raft
        .client_write(authorized(
            Command::Identity { change },
            &a.peer_fingerprint,
        ))
        .await
    {
        Ok(reply) if reply.data.ok => Ok(()),
        Ok(reply) => Err((StatusCode::CONFLICT, Json(reply.data)).into_response()),
        Err(error) => Err(submission_error(a, error)),
    }
}

pub(super) async fn issue_bootstrap(
    State(a): State<App>,
    Json(request): Json<BootstrapRequest>,
) -> Response {
    let Some(issuer) = &a.issuer else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"identity_issuer_not_configured"})),
        )
            .into_response();
    };
    if !converyn_core::model::domain::valid_id(&request.id)
        || !converyn_core::model::domain::valid_id(&request.node_uid)
        || !converyn_core::identities::registration::valid_sha(&request.token_sha256)
        || !(60..=3600).contains(&request.ttl_seconds)
    {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let csr_sha = token_hash(&request.csr_pem);
    if let Some(old) = m.state.registry.bootstraps.get(&request.id) {
        return if old.node_uid == request.node_uid
            && old.token_sha256 == request.token_sha256
            && old.csr_sha256 == csr_sha
            && old.recover == request.recover
        {
            Json(json!({"grant":old})).into_response()
        } else {
            StatusCode::CONFLICT.into_response()
        };
    }
    if !request.recover
        && a.auth
            .as_ref()
            .is_some_and(|auth| auth.agents.contains_key(&request.node_uid))
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error":"static_node_requires_explicit_recovery_grant"})),
        )
            .into_response();
    }
    let subject = format!("bootstrap-{}", &token_hash(&request.node_uid)[..24]);
    let cert = match issuer.sign_client(&subject, &request.csr_pem, request.ttl_seconds) {
        Ok(cert) => cert,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error":"invalid_csr_or_issuer"})),
            )
                .into_response();
        }
    };
    let now = crate::executors::host::now_ms();
    let grant = converyn_core::identities::registration::Bootstrap {
        id: request.id.clone(),
        node_uid: request.node_uid,
        token_sha256: request.token_sha256,
        csr_sha256: csr_sha,
        expires_ms: (now + request.ttl_seconds * 1000).min(cert.expires_ms),
        certificate: cert,
        recover: request.recover,
        consumed: None,
        enrolled_certificate: None,
    };
    if let Err(response) = identity_write(
        &a,
        converyn_core::identities::registration::Change::Bootstrap {
            grant: Box::new(grant),
            now_ms: now,
        },
    )
    .await
    {
        return response;
    }
    match a.store.machine().await {
        Ok(m) => Json(json!({"grant":m.state.registry.bootstraps[&request.id]})).into_response(),
        Err(_) => unavailable(&a),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnrollRequest {
    request_id: String,
    grant_id: String,
    node_uid: String,
    hardware_sha256: String,
    token_sha256: String,
    csr_pem: String,
}

pub(super) async fn enroll_node(
    State(a): State<App>,
    headers: axum::http::HeaderMap,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(request): Json<EnrollRequest>,
) -> Response {
    let Some(issuer) = &a.issuer else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let Some(grant) = m.state.registry.bootstraps.get(&request.grant_id) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if grant.node_uid != request.node_uid
        || grant.token_sha256 != token_hash(token(&headers))
        || grant.certificate.fingerprint != peer.0.0
        || grant.expires_ms <= crate::executors::host::now_ms()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let csr_sha = token_hash(&request.csr_pem);
    let digest = converyn_core::identities::registration::enrollment_digest(
        &request.request_id,
        &request.node_uid,
        &request.hardware_sha256,
        &request.token_sha256,
        &csr_sha,
    );
    if grant.consumed.is_some() {
        return if grant.consumed.as_deref() == Some(&request.request_id)
            && m.state
                .registry
                .nodes
                .get(&request.node_uid)
                .is_some_and(|n| n.enrollment_digest == digest)
        {
            Json(json!({"certificate":grant.enrolled_certificate})).into_response()
        } else {
            StatusCode::CONFLICT.into_response()
        };
    }
    if a.auth.as_ref().is_some_and(|auth| {
        std::iter::once(&auth.admin)
            .chain([&auth.peer])
            .chain(
                auth.agents
                    .iter()
                    .filter(|(uid, _)| *uid != &request.node_uid)
                    .map(|(_, t)| t),
            )
            .any(|t| token_hash(t) == request.token_sha256)
    }) {
        return StatusCode::CONFLICT.into_response();
    }
    let cert = match issuer.sign_client(
        &request.node_uid,
        &request.csr_pem,
        issuer.leaf_lifetime_seconds,
    ) {
        Ok(cert) => cert,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let node = converyn_core::identities::registration::Node {
        uid: request.node_uid.clone(),
        hardware_sha256: request.hardware_sha256,
        token_sha256: request.token_sha256,
        certificate: cert,
        enrollment_id: request.request_id,
        enrollment_digest: digest,
        pending: None,
        retired: false,
    };
    if let Err(response) = identity_write(
        &a,
        converyn_core::identities::registration::Change::Enroll {
            grant_id: request.grant_id.clone(),
            bootstrap_sha256: token_hash(token(&headers)),
            bootstrap_fingerprint: peer.0.0,
            node: Box::new(node),
            now_ms: crate::executors::host::now_ms(),
        },
    )
    .await
    {
        return response;
    }
    match a.store.machine().await {Ok(m)=>Json(json!({"certificate":m.state.registry.bootstraps[&request.grant_id].enrolled_certificate})).into_response(),Err(_)=>unavailable(&a)}
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RenewalRequest {
    request_id: String,
    csr_pem: String,
}

pub(super) async fn renew_node(
    State(a): State<App>,
    ApiPath(node_uid): ApiPath<String>,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(request): Json<RenewalRequest>,
) -> Response {
    let Some(issuer) = &a.issuer else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let Some(node) = m.state.registry.nodes.get(&node_uid) else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error":"node_requires_enrollment"})),
        )
            .into_response();
    };
    let csr_sha = token_hash(&request.csr_pem);
    if let Some(pending) = &node.pending {
        return if pending.id == request.request_id && pending.csr_sha256 == csr_sha {
            Json(json!({"certificate":pending.certificate})).into_response()
        } else {
            StatusCode::CONFLICT.into_response()
        };
    }
    let cert = match issuer.sign_client(&node_uid, &request.csr_pem, issuer.leaf_lifetime_seconds) {
        Ok(cert) => cert,
        Err(_) => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
    };
    let pending = converyn_core::identities::registration::PendingRenewal {
        id: request.request_id,
        csr_sha256: csr_sha,
        certificate: cert,
    };
    if let Err(response) = identity_write(
        &a,
        converyn_core::identities::registration::Change::Renew {
            node_uid: node_uid.clone(),
            pending: Box::new(pending),
            current_fingerprint: peer.0.0,
            now_ms: crate::executors::host::now_ms(),
        },
    )
    .await
    {
        return response;
    }
    match a.store.machine().await {Ok(m)=>Json(json!({"certificate":m.state.registry.nodes[&node_uid].pending.as_ref().map(|p|&p.certificate)})).into_response(),Err(_)=>unavailable(&a)}
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActivateRequest {
    request_id: String,
}

pub(super) async fn activate_renewal(
    State(a): State<App>,
    ApiPath(node_uid): ApiPath<String>,
    peer: axum::Extension<crate::credentials::tls_identity::PeerCertificate>,
    Json(request): Json<ActivateRequest>,
) -> Response {
    match identity_write(
        &a,
        converyn_core::identities::registration::Change::Activate {
            node_uid,
            request_id: request.request_id,
            fingerprint: peer.0.0,
            now_ms: crate::executors::host::now_ms(),
        },
    )
    .await
    {
        Ok(()) => Json(json!({"ok":true})).into_response(),
        Err(r) => r,
    }
}

pub(super) async fn registered_nodes(State(a): State<App>, Query(q): Query<PageQuery>) -> Response {
    let after = match q.after("registered_nodes") {
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
        .registry
        .nodes
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .filter(|(_, n)| {
            q.status
                .as_deref()
                .is_none_or(|s| s == if n.retired { "retired" } else { "active" })
        })
        .take(q.limit + 1)
        .collect::<Vec<_>>();
    let more = rows.len() > q.limit;
    rows.truncate(q.limit);
    let cursor = if more {
        rows.last().map(|(id, _)| q.cursor("registered_nodes", id))
    } else {
        None
    };
    let nodes=rows.into_iter().map(|(_,n)|json!({"uid":n.uid,"hardware_sha256":n.hardware_sha256,"fingerprint":n.certificate.fingerprint,"expires_ms":n.certificate.expires_ms,"retired":n.retired,"renewal_pending":n.pending.is_some()})).collect::<Vec<_>>();
    Json(json!({"nodes":nodes,"next_cursor":cursor})).into_response()
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RetireRequest {
    node_uid: String,
    expected_fingerprint: String,
    evidence: String,
}

pub(super) async fn retire_node(State(a): State<App>, Json(r): Json<RetireRequest>) -> Response {
    match identity_write(
        &a,
        converyn_core::identities::registration::Change::Retire {
            node_uid: r.node_uid,
            expected_fingerprint: r.expected_fingerprint,
            evidence: r.evidence,
        },
    )
    .await
    {
        Ok(()) => Json(json!({"ok":true})).into_response(),
        Err(r) => r,
    }
}
