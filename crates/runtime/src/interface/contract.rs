//! The binary and generated SDK ship the same reviewed wire contract.
pub const OPENAPI: &str = include_str!("../../../../docs/schema/openapi.json");
pub async fn schema() -> impl axum::response::IntoResponse {
    (
        [
            ("content-type", "application/json"),
            ("cache-control", "no-cache"),
        ],
        OPENAPI,
    )
}
