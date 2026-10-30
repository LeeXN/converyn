//! Declared, Run-scoped result contracts and deterministic input materialization.
//! Values are committed with their original receipt; secrets remain sealed in consensus.
use crate::domain::{Operation, Work, digest, valid_id};
use crate::machine::{Run, Status};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
pub const MAX_RECEIPT_BYTES: usize = 256 * 1024;
pub const MAX_RUN_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FIELDS: usize = 128;
fn field_limit() -> usize {
    4096
}
fn lifetime() -> u64 {
    3600
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    String,
    Integer,
    Number,
    Boolean,
    Object,
    Array,
    Null,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Specification {
    #[serde(rename = "type")]
    pub value_type: ValueType,
    #[serde(default)]
    pub secret: bool,
    #[serde(default = "field_limit")]
    pub max_bytes: usize,
    #[serde(default = "lifetime")]
    pub ttl_seconds: u64,
    /// Native/plugin evidence selector; scripts return named fields instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}
pub type Specifications = BTreeMap<String, Specification>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub step: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
    #[serde(default)]
    pub collect: bool,
}
pub type Bindings = BTreeMap<String, Reference>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicKey {
    pub id: String,
    pub public_key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SealedValue {
    pub key_id: String,
    pub ephemeral_key: String,
    pub nonce: String,
    pub ciphertext: String,
    pub plaintext_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoredValue {
    Public { value: Value },
    Secret { sealed: SealedValue },
}
pub type Values = BTreeMap<String, StoredValue>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Materialized {
    pub work: Work,
    pub input_sha256: String,
    pub source_sha256: BTreeMap<String, String>,
}

impl Specification {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=16384).contains(&self.max_bytes),
            "invalid_output_size_limit"
        );
        ensure!(
            !self.secret || (1..=86400).contains(&self.ttl_seconds),
            "invalid_secret_output_lifetime"
        );
        ensure!(
            self.source
                .as_ref()
                .is_none_or(|p| p.starts_with('/') && p.len() <= 256),
            "invalid_output_evidence_selector"
        );
        Ok(())
    }
    pub fn accepts(&self, value: &Value) -> bool {
        let matches = match self.value_type {
            ValueType::String => value.is_string(),
            ValueType::Integer => value.is_i64() || value.is_u64(),
            ValueType::Number => value.is_number(),
            ValueType::Boolean => value.is_boolean(),
            ValueType::Object => value.is_object(),
            ValueType::Array => value.is_array(),
            ValueType::Null => value.is_null(),
        };
        matches
            && serde_json::to_vec(value).is_ok_and(|v| v.len() <= self.max_bytes)
            && !contains_nul(value)
    }
}
fn contains_nul(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains('\0'),
        Value::Array(a) => a.iter().any(contains_nul),
        Value::Object(o) => o.iter().any(|(k, v)| k.contains('\0') || contains_nul(v)),
        _ => false,
    }
}
pub fn validate_specs(specs: &Specifications) -> Result<()> {
    ensure!(specs.len() <= MAX_FIELDS, "too_many_outputs");
    for (name, spec) in specs {
        ensure!(crate::secrets::name_valid(name), "invalid_output_name");
        spec.validate()?;
    }
    ensure!(
        specs.values().map(|s| s.max_bytes).sum::<usize>() <= MAX_DOCUMENT_BYTES,
        "output_contract_too_large"
    );
    Ok(())
}
pub fn no_log(work: &Work) -> bool {
    work.outputs.values().any(|s| s.secret)
}
pub fn validate_plain(specs: &Specifications, values: &BTreeMap<String, Value>) -> Result<()> {
    validate_specs(specs)?;
    ensure!(
        specs.len() == values.len(),
        "output_fields_differ_from_contract"
    );
    for (name, spec) in specs {
        ensure!(
            values.get(name).is_some_and(|v| spec.accepts(v)),
            "output_missing_or_invalid:{name}"
        );
    }
    ensure!(
        serde_json::to_vec(values)?.len() <= MAX_DOCUMENT_BYTES,
        "output_document_too_large"
    );
    Ok(())
}
pub fn validate_stored(
    work: &Work,
    values: &Values,
    key: Option<&PublicKey>,
    succeeded: bool,
) -> Result<()> {
    if !succeeded {
        ensure!(
            values.is_empty(),
            "unsuccessful_work_cannot_publish_outputs"
        );
        return Ok(());
    }
    ensure!(
        work.outputs.len() == values.len(),
        "output_fields_differ_from_contract"
    );
    for (name, spec) in &work.outputs {
        match values.get(name) {
            Some(StoredValue::Public { value }) => {
                ensure!(!spec.secret && spec.accepts(value), "invalid_public_output")
            }
            Some(StoredValue::Secret { sealed }) => {
                ensure!(
                    spec.secret && key.is_some_and(|k| k.id == sealed.key_id),
                    "invalid_secret_output_key"
                );
                ensure!(
                    sealed.plaintext_bytes <= spec.max_bytes,
                    "invalid_secret_output_envelope"
                );
                let ephemeral = STANDARD
                    .decode(&sealed.ephemeral_key)
                    .context("invalid_output_encoding")?;
                let nonce = STANDARD
                    .decode(&sealed.nonce)
                    .context("invalid_output_encoding")?;
                let cipher = STANDARD
                    .decode(&sealed.ciphertext)
                    .context("invalid_output_encoding")?;
                ensure!(
                    ephemeral.len() == 33
                        && matches!(ephemeral.first(), Some(2 | 3))
                        && nonce.len() == 12
                        && cipher.len() == sealed.plaintext_bytes + 16,
                    "invalid_secret_output_envelope"
                );
                ensure!(
                    sealed.plaintext_bytes <= spec.max_bytes
                        && sealed.ephemeral_key.len() == 44
                        && sealed.nonce.len() == 16
                        && sealed.ciphertext.len() <= ((spec.max_bytes + 18) / 3) * 4 + 4,
                    "invalid_secret_output_envelope"
                );
            }
            None => bail!("output_missing"),
        }
    }
    ensure!(
        serde_json::to_vec(values)?.len() <= MAX_DOCUMENT_BYTES * 2,
        "stored_outputs_too_large"
    );
    Ok(())
}

/// Output-derived artifacts would change prefetch/identity reservations. Those
/// remain fixed inputs; data fields reuse the existing binding boundary.
pub fn bindable(operation: &Operation, path: &str) -> bool {
    !matches!(
        operation,
        Operation::Binary { .. } | Operation::Package { .. }
    ) && crate::workflow::bindable(operation, path)
}
fn producers<'a>(
    works: &'a [Work],
    steps: &[crate::domain::Step],
    consumer: &Work,
    reference: &Reference,
) -> Result<Vec<&'a Work>> {
    ensure!(
        valid_id(&reference.step) && crate::secrets::name_valid(&reference.name),
        "invalid_output_reference"
    );
    ensure!(
        reference
            .pointer
            .as_ref()
            .is_none_or(|p| p.starts_with('/') && p.len() <= 256),
        "invalid_output_pointer"
    );
    let matching: Vec<_> = works
        .iter()
        .filter(|w| w.step_id == reference.step)
        .collect();
    ensure!(!matching.is_empty(), "output_producer_not_found");
    let per_node = steps
        .iter()
        .find(|s| s.id == reference.step)
        .context("output_producer_not_found")?
        .scope
        == crate::domain::Scope::PerNode;
    let selected: Vec<_> = matching
        .into_iter()
        .filter(|w| {
            reference.node.as_ref().map_or(
                reference.collect || !per_node || w.node_uid == consumer.node_uid,
                |n| &w.node_uid == n,
            ) && reference.batch.map_or(
                w.batch.is_none() || consumer.batch.is_none() || w.batch == consumer.batch,
                |b| w.batch == Some(b),
            )
        })
        .collect();
    ensure!(
        !selected.is_empty() && (reference.collect || selected.len() == 1),
        "output_reference_ambiguous_or_missing"
    );
    for source in &selected {
        ensure!(
            source.wave < consumer.wave,
            "output_reference_must_point_to_an_earlier_wave"
        );
        ensure!(
            source.outputs.contains_key(&reference.name),
            "output_not_declared"
        );
    }
    Ok(selected)
}
pub fn validate_graph(works: &[Work], steps: &[crate::domain::Step]) -> Result<()> {
    let mut maximum = 0usize;
    for work in works {
        validate_specs(&work.outputs)?;
        ensure!(
            work.outputs
                .values()
                .all(|spec| !spec.secret || matches!(work.operation, Operation::Exec { .. })),
            "sensitive_outputs_require_script_producer"
        );
        ensure!(
            !matches!(work.operation, Operation::Exec { .. })
                || work.outputs.values().all(|spec| spec.source.is_none()),
            "script_outputs_cannot_select_native_evidence"
        );
        maximum = maximum
            .checked_add(work.outputs.values().map(|s| s.max_bytes).sum::<usize>())
            .context("output_budget_overflow")?;
        for (path, reference) in &work.inputs {
            ensure!(
                bindable(&work.operation, path),
                "output_input_field_not_allowed"
            );
            ensure!(
                !work.inputs.keys().any(|other| other != path
                    && (other.starts_with(&(path.clone() + "/"))
                        || path.starts_with(&(other.clone() + "/")))),
                "overlapping_output_bindings"
            );
            let source = producers(works, steps, work, reference)?;
            if source.iter().any(|s| s.outputs[&reference.name].secret) {
                ensure!(
                    matches!(work.operation, Operation::Exec { .. })
                        && path.starts_with("/environment/"),
                    "secret_output_requires_script_environment"
                );
            }
        }
    }
    ensure!(
        maximum <= MAX_RUN_OUTPUT_BYTES,
        "run_output_budget_exceeded"
    );
    Ok(())
}

impl Run {
    pub fn effective_work(&self, id: &str) -> Option<&Work> {
        let declared = self.plan.work.iter().find(|w| w.id == id)?;
        if declared.inputs.is_empty() {
            Some(declared)
        } else {
            self.attempts
                .get(id)?
                .materialized
                .as_ref()
                .map(|m| &m.work)
        }
    }
    pub fn declared_work(&self, id: &str) -> Option<&Work> {
        self.plan.work.iter().find(|w| w.id == id)
    }
    pub fn materializable(&self) -> Vec<String> {
        if self.preparing
            || self.pause_requested
            || self.cancel_requested
            || self.status.terminal()
            || self.status == Status::NeedsAttention
        {
            return Vec::new();
        }
        self.plan
            .work
            .iter()
            .filter(|w| {
                !w.inputs.is_empty()
                    && self.attempts[&w.id].status == Status::Pending
                    && self.attempts[&w.id].materialized.is_none()
                    && self
                        .plan
                        .work
                        .iter()
                        .filter(|p| p.wave < w.wave)
                        .all(|p| self.attempts[&p.id].status == Status::Succeeded)
            })
            .map(|w| w.id.clone())
            .collect()
    }
    pub fn materialize(
        &self,
        id: &str,
        now_ms: u64,
        contracts: &dyn converyn_plugin_api::Contracts,
    ) -> Result<(Materialized, BTreeMap<String, String>)> {
        let declared = self.declared_work(id).context("work_not_found")?;
        let mut work = declared.clone();
        let mut source_sha256 = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        for (path, reference) in &declared.inputs {
            let sources = producers(
                &self.plan.work,
                &self.plan.input.playbook.steps,
                declared,
                reference,
            )?;
            let mut values = Vec::new();
            let mut secret = false;
            for source in &sources {
                let attempt = &self.attempts[&source.id];
                ensure!(
                    attempt.status == Status::Succeeded,
                    "output_producer_not_succeeded"
                );
                let output = attempt
                    .receipt
                    .as_ref()
                    .and_then(|r| r.outputs.get(&reference.name))
                    .context("output_missing")?;
                let spec = &source.outputs[&reference.name];
                secret |= spec.secret;
                if spec.secret {
                    let created = attempt
                        .completed_ms
                        .context("secret_output_has_no_commit_time")?;
                    ensure!(
                        now_ms < created.saturating_add(spec.ttl_seconds * 1000),
                        "secret_output_expired"
                    );
                }
                let value = match output {
                    StoredValue::Public { value } => reference
                        .pointer
                        .as_ref()
                        .map_or(Some(value), |p| value.pointer(p))
                        .context("output_pointer_missing")?
                        .clone(),
                    StoredValue::Secret { .. } => Value::Null,
                };
                values.push(serde_json::json!({"node_uid":source.node_uid,"batch":source.batch,"value":value}));
            }
            let fingerprint = digest(
                &sources
                    .iter()
                    .map(|s| {
                        (
                            &s.id,
                            &self.attempts[&s.id].receipt.as_ref().unwrap().outputs
                                [&reference.name],
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            source_sha256.insert(path.clone(), fingerprint.clone());
            if secret {
                let Operation::Exec { environment, .. } = &mut work.operation else {
                    bail!("secret_output_requires_script_environment")
                };
                let name = path
                    .strip_prefix("/environment/")
                    .context("invalid_secret_binding")?;
                environment.insert(
                    name.into(),
                    crate::secrets::Value::OutputSecret(
                        converyn_plugin_api::variables::OutputSecretReference {
                            output_secret: converyn_plugin_api::variables::OutputSecretSource {
                                run_id: self.id.clone(),
                                step: reference.step.clone(),
                                name: reference.name.clone(),
                                work_ids: sources.iter().map(|s| s.id.clone()).collect(),
                                pointer: reference.pointer.clone(),
                                collect: reference.collect,
                                sha256: fingerprint,
                            },
                        },
                    ),
                );
                continue;
            }
            let value = if reference.collect {
                Value::Array(values)
            } else {
                values.remove(0)["value"].take()
            };
            apply_value(
                &mut work.operation,
                path,
                value,
                &mut definitions,
                contracts,
            )?;
        }
        work.operation.validate()?;
        ensure!(
            work.operation.resource_keys(&work.node_uid)
                == declared.operation.resource_keys(&declared.node_uid),
            "output_input_cannot_change_resource_identity"
        );
        let input_sha256 = digest(&(&work.operation, &source_sha256));
        work.input_sha256 = Some(input_sha256.clone());
        Ok((
            Materialized {
                work,
                input_sha256,
                source_sha256,
            },
            definitions,
        ))
    }
}
fn apply_value(
    operation: &mut Operation,
    path: &str,
    value: Value,
    definitions: &mut BTreeMap<String, String>,
    contracts: &dyn converyn_plugin_api::Contracts,
) -> Result<()> {
    ensure!(bindable(operation, path), "output_input_field_not_allowed");
    match operation {
        Operation::Exec { environment, .. } => {
            let name = path
                .strip_prefix("/environment/")
                .context("invalid_environment_binding")?;
            environment.insert(
                name.into(),
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
                    .into(),
            );
        }
        Operation::File {
            path: destination,
            sha256,
            ..
        } => {
            let text = value.as_str().context("file_output_input_must_be_string")?;
            ensure!(text.len() <= 65536, "materialized_definition_too_large");
            if destination.ends_with(".json") {
                serde_json::from_str::<Value>(text)
                    .map_err(|_| anyhow::anyhow!("invalid_configuration_json"))?;
            }
            *sha256 = crate::workflow::content_sha256(text);
            definitions.insert(sha256.clone(), text.into());
        }
        _ => {
            let mut encoded = serde_json::to_value(&*operation)?;
            if let Some(slot) = encoded.pointer_mut(path) {
                *slot = value;
            } else {
                let (parent, key) = path.rsplit_once('/').context("output_input_path_missing")?;
                encoded
                    .pointer_mut(parent)
                    .and_then(Value::as_object_mut)
                    .context("output_input_path_missing")?
                    .insert(key.into(), value);
            }
            *operation = serde_json::from_value(encoded)?;
            if let Operation::Plugin { call } = operation {
                contracts.validate_call(call)?;
            }
        }
    }
    Ok(())
}
