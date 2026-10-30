//! The UI is compiled into the distributable binary. It stores no credentials;
//! API requests use the caller's mTLS certificate and in-memory bearer token.
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

fn version() -> &'static str {
    static VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let mut hash = Sha256::new();
        hash.update(include_bytes!("../../../../web/dist/assets/app.js"));
        hash.update(include_bytes!("../../../../web/dist/assets/index.css"));
        hex::encode(hash.finalize())
    });
    &VERSION
}

pub async fn version_info() -> Response {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        axum::Json(serde_json::json!({"version": version()})),
    )
        .into_response()
}
pub async fn index() -> Response {
    let html = include_str!("../../../../web/dist/index.html")
        .replace("/assets/app.js", &format!("/assets/app.js?v={}", version()))
        .replace(
            "/assets/index.css",
            &format!("/assets/index.css?v={}", version()),
        )
        .replace(
            "</head>",
            &format!(
                "<meta name=\"converyn-ui-version\" content=\"{}\"></head>",
                version()
            ),
        );
    (asset_headers("text/html; charset=utf-8"), html).into_response()
}
pub async fn javascript() -> Response {
    asset(
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/dist/assets/app.js"),
    )
}
pub async fn stylesheet() -> Response {
    asset(
        "text/css; charset=utf-8",
        include_bytes!("../../../../web/dist/assets/index.css"),
    )
}
fn asset(kind: &'static str, bytes: &'static [u8]) -> Response {
    (StatusCode::OK, asset_headers(kind), bytes).into_response()
}
fn asset_headers(kind: &'static str) -> [(header::HeaderName, &'static str); 5] {
    [
        (header::CONTENT_TYPE, kind),
        (header::CACHE_CONTROL, "no-cache"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
        (header::REFERRER_POLICY, "no-referrer"),
    ]
}
