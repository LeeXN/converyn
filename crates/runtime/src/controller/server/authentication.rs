//! mTLS、角色、原始调用者转发与作用域验证。
use super::*;

pub(super) async fn authenticate(
    State(app): State<App>,
    mut request: axum::extract::Request,
    next: Next,
) -> Response {
    if (request.uri().path().starts_with("/api/v1/agents/")
        || request.uri().path() == "/api/v1/identity/enroll")
        && request
            .headers()
            .get("x-converyn-protocol")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.parse::<u64>().ok())
            != Some(crate::controller::network::PROTOCOL_VERSION)
    {
        return (StatusCode::UPGRADE_REQUIRED,Json(json!({"error":"incompatible_agent_protocol","required":crate::controller::network::PROTOCOL_VERSION}))).into_response();
    }
    if request.uri().path().starts_with("/internal/raft/")
        && request
            .headers()
            .get("x-converyn-protocol")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            != Some(crate::controller::network::PROTOCOL_VERSION)
    {
        return (StatusCode::UPGRADE_REQUIRED,Json(json!({"error":"incompatible_peer_protocol","required":crate::controller::network::PROTOCOL_VERSION}))).into_response();
    }
    if request.uri().path().starts_with("/internal/raft/")
        && request
            .headers()
            .get("x-converyn-plugin-manifest")
            .and_then(|h| h.to_str().ok())
            != Some(crate::plugins::registry().fingerprint().as_str())
    {
        return (
            StatusCode::UPGRADE_REQUIRED,
            Json(json!({"error":"incompatible_plugin_contracts"})),
        )
            .into_response();
    }
    if app.https
        && request
            .extensions()
            .get::<crate::credentials::tls_identity::PeerCertificate>()
            .is_none_or(|peer| peer.1 <= crate::executors::host::now_ms())
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if request
        .headers()
        .contains_key("x-converyn-forwarded-bearer")
        || request
            .headers()
            .contains_key("x-converyn-forwarded-fingerprint")
    {
        if !request.uri().path().starts_with("/api/v1/") {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let Some(auth) = &app.auth else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        let actual = request
            .extensions()
            .get::<crate::credentials::tls_identity::PeerCertificate>()
            .cloned();
        let token = request
            .headers()
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .unwrap_or("");
        let Ok(m) = app.store.machine().await else {
            return unavailable(&app);
        };
        let trusted = auth.principal(token).as_deref() == Some("peer")
            && actual.as_ref().is_some_and(|peer| {
                m.state.identities.bindings.get("peer").map_or_else(
                    || auth.bound(token, &peer.0),
                    |b| b.fingerprints.contains(&peer.0),
                )
            });
        let original = request
            .headers()
            .get("x-converyn-forwarded-bearer")
            .cloned();
        let fingerprint = request
            .headers()
            .get("x-converyn-forwarded-fingerprint")
            .and_then(|h| h.to_str().ok())
            .map(String::from);
        let expiry = request
            .headers()
            .get("x-converyn-forwarded-expiry")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        if !trusted
            || original.is_none()
            || fingerprint
                .as_ref()
                .is_none_or(|s| !converyn_core::identities::registration::valid_sha(s))
            || expiry.is_none_or(|t| t <= crate::executors::host::now_ms())
        {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let mut original = original.unwrap();
        original.set_sensitive(true);
        request.headers_mut().insert("authorization", original);
        request.headers_mut().remove("x-converyn-forwarded-bearer");
        request
            .extensions_mut()
            .insert(crate::credentials::tls_identity::PeerCertificate(
                fingerprint.unwrap(),
                expiry.unwrap(),
            ));
    }
    if request.uri().path().starts_with("/api/v1/")
        && request.uri().path() != "/api/v1/controlplane/metrics"
        && request.uri().path() != "/api/v1/controlplane/prometheus"
    {
        let leader = app.raft.metrics().borrow().current_leader;
        if leader.is_some_and(|id| id != app.id) {
            if request.headers().contains_key("x-converyn-forward-hop") {
                return unavailable(&app);
            }
            return forward_api(&app, request).await;
        }
    }
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let mut authorization = None;
    let path = request.uri().path();
    let authorized = if let Some(auth) = &app.auth {
        if path == "/api/v1/identity/enroll" {
            if !confirm_read(&app).await {
                return unavailable(&app);
            }
            match (
                request
                    .extensions()
                    .get::<crate::credentials::tls_identity::PeerCertificate>(),
                app.store.machine().await,
            ) {
                (Some(peer), Ok(m)) => m.state.registry.bootstraps.values().any(|g| {
                    g.token_sha256 == token_hash(bearer)
                        && g.certificate.fingerprint == peer.0
                        && g.expires_ms > crate::executors::host::now_ms()
                }),
                _ => false,
            }
        } else {
            let mut principal = auth.principal(bearer);
            let bootstrap = path == "/api/v1/controlplane/init"
                || path == "/api/v1/controlplane/metrics"
                || path == "/api/v1/controlplane/prometheus";
            if principal.as_deref() != Some("peer") && !bootstrap && !confirm_read(&app).await {
                return unavailable(&app);
            }
            let Ok(m) = app.store.machine().await else {
                return unavailable(&app);
            };
            if principal.is_none() {
                principal = m
                    .state
                    .registry
                    .token_index
                    .get(&token_hash(bearer))
                    .map(|uid| format!("node:{uid}"));
            }
            let personal = request
                .extensions()
                .get::<crate::credentials::tls_identity::PeerCertificate>()
                .and_then(|peer| {
                    m.state.personnel.authenticate(
                        &peer.0,
                        &token_hash(bearer),
                        crate::executors::host::now_ms(),
                    )
                });
            if let Some((identity, _)) = personal {
                principal = Some(identity.uid.clone());
            }
            if let (Some(principal), Some(peer)) = (
                &principal,
                request
                    .extensions()
                    .get::<crate::credentials::tls_identity::PeerCertificate>(),
            ) {
                authorization = Some(converyn_core::identities::registration::Authorization {
                    request_id: request_id(),
                    observed_ms: Some(crate::executors::host::now_ms()),
                    token_sha256: Some(token_hash(bearer)),
                    principal: principal.clone(),
                    fingerprint: peer.0.clone(),
                });
            }
            let node = principal.as_ref().and_then(|p| p.strip_prefix("node:"));
            let registry_node = node.and_then(|uid| m.state.registry.nodes.get(uid));
            let credential_current =
                registry_node.is_none_or(|n| !n.retired && n.token_sha256 == token_hash(bearer));
            let allowed = if let Some((identity, _)) = personal {
                personal_path_allowed(&m.state, &identity.uid, request.method(), path)
            } else if principal.as_deref() == Some("admin") {
                [
                    "/api/v1/identity/me",
                    "/health/ready",
                    "/api/v1/controlplane/init",
                    "/api/v1/controlplane/metrics",
                    "/api/v1/controlplane/prometheus",
                ]
                .contains(&path)
                    || (!m
                        .state
                        .personnel
                        .has_active_admin(crate::executors::host::now_ms())
                        && (["/api/v1/identities", "/api/v1/credentials"].contains(&path)
                            || path.starts_with("/api/v1/credentials/")))
            } else if let Some(uid) = node {
                credential_current && path.starts_with(&format!("/api/v1/agents/{uid}/"))
            } else if principal.as_deref() == Some("peer") {
                path.starts_with("/internal/raft/")
            } else {
                false
            };
            let bound = if personal.is_some() || !app.https {
                true
            } else if let Some(peer) = request
                .extensions()
                .get::<crate::credentials::tls_identity::PeerCertificate>()
            {
                principal.as_ref().is_some_and(|p| {
                    m.state.identities.bindings.get(p).map_or_else(
                        || auth.bound(bearer, &peer.0),
                        |b| b.fingerprints.contains(&peer.0),
                    )
                })
            } else {
                false
            };
            let identity_ok = if let Some(uid) = node {
                let observation = m.state.observations.get(uid);
                let hardware = registry_node
                    .map(|n| n.hardware_sha256.as_str())
                    .or_else(|| observation.and_then(|o| o.hardware_sha256.as_deref()));
                let matches_hardware = hardware.is_none_or(|expected| {
                    request
                        .headers()
                        .get("x-converyn-hardware")
                        .and_then(|v| v.to_str().ok())
                        == Some(expected)
                });
                let needs_session = hardware.is_some()
                    && !path.ends_with("/heartbeat")
                    && !path.ends_with("/renew")
                    && !path.ends_with("/renew/activate");
                matches_hardware
                    && (!needs_session
                        || observation.is_some_and(|o| {
                            request
                                .headers()
                                .get("x-converyn-session")
                                .and_then(|v| v.to_str().ok())
                                == Some(o.session_id.as_str())
                        }))
            } else {
                true
            };
            if !allowed && bound && identity_ok && credential_current && principal.is_some() {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error":"permission_denied","message":"This identity has no permission for this action or scope. Check converyn auth status or contact an access administrator."})),
                )
                    .into_response();
            }
            allowed && bound && identity_ok
        }
    } else {
        bearer == app.token
    };
    if !authorized
        || request
            .headers()
            .get("x-converyn-cluster")
            .and_then(|v| v.to_str().ok())
            != Some(app.cluster.as_str())
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        )
            .into_response();
    }
    let mut response = if let Some(authorization) = authorization {
        REQUEST_AUTH.scope(authorization, next.run(request)).await
    } else {
        next.run(request).await
    };
    response.headers_mut().insert(
        "x-converyn-node",
        HeaderValue::from_str(&app.id.to_string()).unwrap(),
    );
    response
}

/// Browser requests stay on the chosen origin. A follower streams to the
/// verified leader; only a trusted peer may attest the original TLS identity.
pub(super) async fn forward_api(app: &App, request: axum::extract::Request) -> Response {
    let metrics = app.raft.metrics().borrow().clone();
    let Some(id) = metrics.current_leader else {
        return unavailable(app);
    };
    let Some(node) = metrics
        .membership_config
        .membership()
        .get_node(&id)
        .cloned()
    else {
        return unavailable(app);
    };
    let peer_token = app
        .auth
        .as_ref()
        .map(|a| a.peer.clone())
        .unwrap_or_else(|| app.token.clone());
    let fingerprints = app
        .auth
        .as_ref()
        .and_then(|a| a.certificates.get("peer"))
        .cloned()
        .unwrap_or_default();
    let network = Network {
        https: app.https,
        token: peer_token.clone(),
        cluster: app.cluster.clone(),
        client: app.peer_http.clone(),
        store: Some(app.store.clone()),
        peer_fingerprints: fingerprints,
    };
    if network
        .verify_target(id, &node, Duration::from_secs(3))
        .await
        .is_err()
    {
        return unavailable(app);
    }
    let original = request
        .headers()
        .get("authorization")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("Bearer "));
    let identity = request
        .extensions()
        .get::<crate::credentials::tls_identity::PeerCertificate>()
        .cloned();
    let (parts, body) = request.into_parts();
    let uri = parts
        .uri
        .path_and_query()
        .map(|u| u.as_str())
        .unwrap_or("/");
    let mut headers = parts.headers;
    // Exactly one transport Authorization header is sent. The original
    // credential is carried separately and rebound to its original TLS leaf.
    headers.remove("authorization");
    for key in [
        "host",
        "connection",
        "transfer-encoding",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "upgrade",
    ] {
        headers.remove(key);
    }
    headers.insert("x-converyn-forward-hop", HeaderValue::from_static("1"));
    if app.https {
        let Some(identity) = identity else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        let mut original = original.clone();
        original.set_sensitive(true);
        headers.insert("x-converyn-forwarded-bearer", original);
        headers.insert(
            "x-converyn-forwarded-fingerprint",
            HeaderValue::from_str(&identity.0).unwrap(),
        );
        headers.insert(
            "x-converyn-forwarded-expiry",
            HeaderValue::from_str(&identity.1.to_string()).unwrap(),
        );
    }
    let response = app
        .peer_http
        .request(
            parts.method,
            format!(
                "{}://{}{uri}",
                if app.https { "https" } else { "http" },
                node.addr
            ),
        )
        .headers(headers)
        .bearer_auth(if app.https {
            &peer_token
        } else {
            original
                .to_str()
                .unwrap_or("")
                .strip_prefix("Bearer ")
                .unwrap_or("")
        })
        .timeout(Duration::from_secs(3600))
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await;
    let Ok(response) = response else {
        return unavailable(app);
    };
    let status = response.status();
    let mut headers = response.headers().clone();
    for key in [
        "connection",
        "transfer-encoding",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "upgrade",
    ] {
        headers.remove(key);
    }
    let mut output =
        axum::response::Response::new(axum::body::Body::from_stream(response.bytes_stream()));
    *output.status_mut() = status;
    *output.headers_mut() = headers;
    output
}

pub(super) fn token(headers: &axum::http::HeaderMap) -> &str {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("")
}

pub(super) fn token_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(value))
}

pub(super) async fn node_proof(
    a: &App,
    node: &str,
    headers: &axum::http::HeaderMap,
    peer: &crate::credentials::tls_identity::PeerCertificate,
) -> Result<converyn_core::identities::registration::Proof, Response> {
    let bearer = token(headers);
    let hash = token_hash(bearer);
    let m = a.store.machine().await.map_err(|_| unavailable(a))?;
    let principal = a
        .auth
        .as_ref()
        .and_then(|auth| auth.principal(bearer))
        .or_else(|| {
            m.state
                .registry
                .token_index
                .get(&hash)
                .map(|uid| format!("node:{uid}"))
        });
    if principal.as_deref() != Some(&format!("node:{node}")) {
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    Ok(converyn_core::identities::registration::Proof {
        principal: principal.unwrap(),
        fingerprint: peer.0.clone(),
        token_sha256: hash,
        hardware_sha256: headers
            .get("x-converyn-hardware")
            .and_then(|h| h.to_str().ok())
            .map(String::from),
        session: headers
            .get("x-converyn-session")
            .and_then(|h| h.to_str().ok())
            .map(String::from),
    })
}
