//! Workflow-owned values are frozen into each node's environment before planning.
//! Objects merge recursively; arrays and scalars replace the previous value.
use anyhow::{Result, ensure};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub type Values = BTreeMap<String, Value>;
pub const JSON_ENV: &str = "WORKFLOW_VALUES_JSON";

pub fn merge(base: &mut Value, overlay: &Value) {
    if let (Value::Object(base), Value::Object(overlay)) = (&mut *base, overlay) {
        for (key, value) in overlay {
            merge(base.entry(key).or_insert(Value::Null), value);
        }
    } else {
        *base = overlay.clone();
    }
}
pub fn overlay(base: &mut Values, values: &Values) {
    for (key, value) in values {
        merge(base.entry(key.clone()).or_insert(Value::Null), value);
    }
}
pub fn pointer(path: &str) -> String {
    format!(
        "/{}",
        path.split('.')
            .map(|s| s.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    )
}
pub fn environment_allowed(name: &str) -> bool {
    crate::secrets::name_valid(name)
        && super::workflow::safe_environment(name)
        && name != JSON_ENV
        && ![
            "KUBECONFIG",
            "DOCKER_HOST",
            "DOCKER_CONTEXT",
            "CONTAINER_HOST",
            "SSH_AUTH_SOCK",
            "AWS_PROFILE",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "AZURE_CLIENT_SECRET",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ]
        .contains(&name)
}
pub fn validate(values: &Values) -> Result<()> {
    ensure!(values.len() <= 48, "too_many_workflow_values");
    fn data(value: &Value, depth: usize) -> bool {
        depth <= 16
            && match value {
                Value::String(s) => !s.contains('\0'),
                Value::Array(a) => a.len() <= 256 && a.iter().all(|v| data(v, depth + 1)),
                Value::Object(o) => {
                    o.len() <= 256
                        && o.iter()
                            .all(|(k, v)| !k.is_empty() && !k.contains('\0') && data(v, depth + 1))
                }
                _ => true,
            }
    }
    ensure!(
        values
            .iter()
            .all(|(name, value)| environment_allowed(name) && data(value, 0)),
        "invalid_workflow_value"
    );
    // Keep the complete snapshot bounded before multiplying it across nodes.
    ensure!(
        serde_json::to_vec(values)?.len() <= 65536,
        "workflow_values_exceed_64_kib"
    );
    Ok(())
}
pub fn resolve(
    defaults: &Values,
    node: &Values,
    overrides: &Values,
    immutable: &BTreeSet<String>,
) -> Result<Values> {
    let mut baseline = defaults.clone();
    overlay(&mut baseline, node);
    let mut result = baseline.clone();
    overlay(&mut result, overrides);
    let before = serde_json::to_value(&baseline)?;
    let after = serde_json::to_value(&result)?;
    for path in immutable {
        let path_pointer = pointer(path);
        ensure!(
            before.pointer(&path_pointer) == after.pointer(&path_pointer),
            "immutable_workflow_value:{path}"
        );
    }
    validate(&result)?;
    Ok(result)
}
pub fn environment(values: &Values) -> Result<crate::secrets::Variables> {
    let mut result: crate::secrets::Variables = values
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                crate::secrets::Value::Literal(
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                ),
            )
        })
        .collect();
    result.insert(JSON_ENV.into(), serde_json::to_string(values)?.into());
    Ok(result)
}

/// An equivalent invocation view: literals are shell quoted, secrets remain runtime references.
/// Never expand user data into the script source or evaluate it as a template.
pub fn render(script: &str, environment: &crate::secrets::Variables) -> String {
    let mut view =
        String::from("# Converyn execution environment (Secret values resolved only on Agent)\n");
    for (name, value) in environment {
        match value {
            crate::secrets::Value::Literal(value) => view.push_str(&format!(
                "export {name}='{}'\n",
                value.replace('\'', "'\\''")
            )),
            crate::secrets::Value::OutputSecret(reference) => view.push_str(&format!(
                "# {name}=<Run output secret:{}/{}>\n",
                reference.output_secret.step, reference.output_secret.name
            )),
            crate::secrets::Value::Secret(reference) => view.push_str(&format!(
                "# {name}=<secret:{}/{}>\n",
                reference.secret, reference.version
            )),
        }
    }
    view.push_str("\n# Original script\n");
    view.push_str(script);
    view
}
