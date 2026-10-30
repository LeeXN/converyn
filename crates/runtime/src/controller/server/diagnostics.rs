//! Correlation IDs contain no credentials. A forwarded request retains its ID
//! while authorization continues to depend exclusively on verified identities.
use super::*;
use tracing::Instrument;

tokio::task_local! { pub(super) static REQUEST_ID: String; }
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

pub(super) fn request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok()
}

pub(super) async fn trace_request(mut request: axum::extract::Request, next: Next) -> Response {
    let id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{:x}-{:x}-{:x}",
                crate::executors::host::now_ms(),
                std::process::id(),
                NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
            )
        });
    request
        .headers_mut()
        .insert("x-request-id", HeaderValue::from_str(&id).unwrap());
    // Log path rather than URI: query strings and Authorization headers are not
    // telemetry and may contain caller-provided sensitive material.
    let business =
        request.uri().path().starts_with("/api/v1/") || request.uri().path() == "/health/ready";
    let span = tracing::info_span!("http_request",request_id=%id,method=%request.method(),path=%request.uri().path());
    let start = std::time::Instant::now();
    let mut response = REQUEST_ID
        .scope(id.clone(), next.run(request).instrument(span.clone()))
        .await;
    if business && response.status().is_client_error()
        || business && response.status().is_server_error()
    {
        let status = response.status();
        let (mut parts, body) = response.into_parts();
        let raw = axum::body::to_bytes(body, 65536).await.unwrap_or_default();
        let mut value = serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()
            .filter(|v| v.is_object())
            .unwrap_or_else(|| json!({}));
        let fallback = match status.as_u16() {
            400 => "invalid_request",
            401 => "unauthorized",
            403 => "forbidden",
            404 => "not_found",
            409 => "conflict",
            413 => "request_too_large",
            415 => "unsupported_media_type",
            422 => "validation_failed",
            429 => "write_queue_full",
            503 => "temporarily_unavailable",
            _ => "request_failed",
        };
        if value.get("code").is_none() {
            value["code"] = value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or(fallback)
                .into();
        }
        if value.get("message").is_none() {
            value["message"] = status.canonical_reason().unwrap_or("Request failed").into();
        }
        if value.get("retryable").is_none() {
            value["retryable"] = (status.as_u16() == 429 || status.as_u16() == 503).into();
        }
        value["request_id"] = id.clone().into();
        parts.headers.remove("content-length");
        parts
            .headers
            .insert("content-type", HeaderValue::from_static("application/json"));
        response = Response::from_parts(
            parts,
            axum::body::Body::from(serde_json::to_vec(&value).unwrap()),
        );
    }
    response
        .headers_mut()
        .insert("x-request-id", HeaderValue::from_str(&id).unwrap());
    tracing::debug!(parent: &span,status=response.status().as_u16(),elapsed_ms=start.elapsed().as_millis(),"request completed");
    response
}
