//! Persistent encrypted secrets; plaintext is confined to authenticated input
//! handling and the existing grant-scoped Work input response.
use super::*;
use crate::credentials::{outputs::Keyring, secrets as secret};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegisterSecret {
    name: String,
    version: String,
    value: String,
}
pub(super) async fn register_secret(
    State(a): State<App>,
    Json(input): Json<RegisterSecret>,
) -> Response {
    use converyn_core::model::domain::valid_id;
    if !a.https || a.auth.is_none() {
        return (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"code":"secret_requires_mtls","message":"Secret registration requires the authenticated mTLS control plane"}))).into_response();
    }
    if !valid_id(&input.name)
        || !valid_id(&input.version)
        || input.value.is_empty()
        || input.value.contains('\0')
        || serde_json::to_vec(&input.value).map_or(true, |v| v.len() > 16384)
    {
        return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"code":"invalid_secret","message":"Valid name/version and nonempty UTF-8 value up to 16 KiB JSON are required"}))).into_response();
    }
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    if !request_allows(
        &m.state,
        converyn_core::identities::access::Action::Platform,
        None,
        None,
    ) {
        return denied("platform");
    }
    let Some(path) = &a.output_keyring else {
        return unavailable(&a);
    };
    let Ok(keys) = Keyring::load(path) else {
        return unavailable(&a);
    };
    if let Some(old) = m
        .state
        .administration
        .secrets
        .get(&input.name)
        .and_then(|v| v.get(&input.version))
    {
        return match secret::open(&keys, &input.name, &input.version, old) {
            Ok(value) if value == input.value => Json(
                json!({"ok":true,"code":"secret_already_registered","run_id":null,"revision":null}),
            )
            .into_response(),
            Ok(_) => (
                StatusCode::CONFLICT,
                Json(json!({"code":"secret_version_immutable"})),
            )
                .into_response(),
            Err(_) => unavailable(&a),
        };
    }
    let Ok(sealed) = crate::credentials::outputs::seal(
        &keys.public_key(),
        &json!(input.value),
        &secret::binding(&input.name, &input.version),
    ) else {
        return unavailable(&a);
    };
    command(
        State(a),
        Json(Command::Administer {
            change: converyn_core::orchestration::administration::Change::Secret {
                name: input.name,
                version: input.version,
                sealed,
            },
        }),
    )
    .await
}
pub(super) async fn secret_catalog(State(a): State<App>) -> Response {
    if !confirm_read(&a).await {
        return unavailable(&a);
    }
    let Ok(m) = a.store.machine().await else {
        return unavailable(&a);
    };
    let rows: Vec<_> = m
        .state
        .administration
        .secrets
        .iter()
        .flat_map(|(name, versions)| {
            versions
                .keys()
                .map(move |version| json!({"name":name,"version":version}))
        })
        .collect();
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(json!({"secrets":rows})),
    )
        .into_response()
}
