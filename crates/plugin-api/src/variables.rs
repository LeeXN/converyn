//! 参数与 Secret 的不可变版本引用；解析后的值只存在于本地运行时。
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub secret: String,
    pub version: String,
}
/// A sealed producer result, generated only by committed Run materialization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputSecretSource {
    pub run_id: String,
    pub step: String,
    pub name: String,
    pub work_ids: Vec<String>,
    pub pointer: Option<String>,
    pub collect: bool,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputSecretReference {
    pub output_secret: OutputSecretSource,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Value {
    Literal(String),
    Secret(Reference),
    OutputSecret(OutputSecretReference),
}
impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Literal(value.into())
    }
}
pub type Variables = BTreeMap<String, Value>;
pub fn name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && (name.as_bytes()[0].is_ascii_alphabetic() || name.starts_with('_'))
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.starts_with("CONVERYN_")
}
pub fn validate(values: &Variables) -> Result<()> {
    ensure!(values.len() <= 64, "too many environment bindings");
    for (name, value) in values {
        ensure!(name_valid(name), "invalid or reserved environment name");
        match value {
            Value::Literal(value) => ensure!(
                value.len() <= 65536 && !value.contains('\0'),
                "invalid literal environment value"
            ),
            Value::OutputSecret(r) => ensure!(
                crate::valid_id(&r.output_secret.run_id)
                    && crate::valid_id(&r.output_secret.step)
                    && name_valid(&r.output_secret.name)
                    && !r.output_secret.work_ids.is_empty()
                    && r.output_secret.work_ids.len() <= 300
                    && r.output_secret
                        .work_ids
                        .iter()
                        .all(|id| crate::valid_id(id))
                    && r.output_secret.sha256.len() == 64,
                "invalid Run output secret reference"
            ),
            Value::Secret(r) => ensure!(
                crate::valid_id(&r.secret) && crate::valid_id(&r.version),
                "invalid secret reference"
            ),
        }
    }
    ensure!(
        serde_json::to_vec(values)?.len() <= 128 * 1024,
        "environment description exceeds limit"
    );
    Ok(())
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Resolved {
    pub values: BTreeMap<String, String>,
    pub redactions: Vec<Vec<u8>>,
}

impl std::fmt::Debug for Resolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolved")
            .field("names", &self.values.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}
