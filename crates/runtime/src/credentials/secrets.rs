//! Central encrypted Secret references and ephemeral task input materialization.
use anyhow::{Context, Result, ensure};
pub use converyn_plugin_api::variables::*;

/// Secret references are resolved only by the authenticated Work input protocol.
/// This helper cannot fall back to a node-local credential store.
pub fn resolve_literals(values: &Variables) -> Result<Resolved> {
    validate(values)?;
    let mut resolved = Resolved::default();
    for (name, value) in values {
        let Value::Literal(value) = value else {
            anyhow::bail!("Secret references require the current Work grant")
        };
        resolved.values.insert(name.clone(), value.clone());
    }
    ensure!(
        resolved.values.values().map(String::len).sum::<usize>() <= 128 * 1024,
        "resolved environment exceeds 128 KiB"
    );
    Ok(resolved)
}

/// Domain separation keeps stored credentials distinct from step outputs even
/// though both use the installation's independently backed-up encryption keys.
pub fn binding(name: &str, version: &str) -> Vec<u8> {
    serde_json::to_vec(&("converyn-stored-secret-v1", name, version)).expect("secret binding")
}
pub fn specification() -> converyn_core::model::outputs::Specification {
    converyn_core::model::outputs::Specification {
        value_type: converyn_core::model::outputs::ValueType::String,
        secret: true,
        max_bytes: 16384,
        ttl_seconds: 3600,
        source: None,
    }
}
pub fn open(
    keys: &super::outputs::Keyring,
    name: &str,
    version: &str,
    sealed: &converyn_core::model::outputs::SealedValue,
) -> Result<String> {
    keys.open(sealed, &binding(name, version), &specification())?
        .as_str()
        .map(str::to_owned)
        .context("invalid stored secret")
}

pub fn environment(operation: &converyn_core::model::domain::Operation) -> Result<Variables> {
    use converyn_core::model::domain::Operation;
    match operation {
        Operation::Exec { environment, .. } => Ok(environment.clone()),
        Operation::Plugin { call } => {
            let mut variables: Variables = call
                .spec
                .get("environment")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?
                .unwrap_or_default();
            if call.plugin.id == "converyn.docker"
                && let Some(reference) = call.spec.get("credential_ref").filter(|v| !v.is_null())
            {
                variables.insert(
                    "CONVERYN_DOCKER_AUTH".into(),
                    Value::Secret(serde_json::from_value(reference.clone())?),
                );
            }
            Ok(variables)
        }
        _ => Ok(Default::default()),
    }
}
