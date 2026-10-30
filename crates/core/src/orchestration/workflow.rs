//! Registered workflows are the only public execution inputs. Compilation and
//! admission repeat inside consensus so a stale preview cannot change its scope.
use crate::{
    domain::{Operation, Plan, PlanInput, Playbook, digest, valid_id},
    machine::{Command, Reply, State},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TargetPreset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<String>,
    #[serde(default)]
    pub owner_uid: Option<String>,
    pub node_group_uid: Option<String>,
    pub external_target_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub step_id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<BindingTransform>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_json_paths: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scalar_rules: BTreeMap<String, ScalarRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScalarRule {
    Integer { minimum: i64, maximum: i64 },
    Quantity,
    Duration,
}
impl ScalarRule {
    fn accepts(&self, value: &Value) -> bool {
        match self {
            Self::Integer { minimum, maximum } => value
                .as_i64()
                .is_some_and(|n| (*minimum..=*maximum).contains(&n)),
            Self::Quantity => value.as_str().is_some_and(|s| {
                if s.is_empty() || s.len() > 32 {
                    return false;
                }
                let split = s
                    .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                    .unwrap_or(s.len());
                s[..split]
                    .parse::<f64>()
                    .is_ok_and(|n| n.is_finite() && n > 0.0)
                    && [
                        "", "n", "u", "m", "k", "M", "G", "T", "P", "E", "Ki", "Mi", "Gi", "Ti",
                        "Pi", "Ei",
                    ]
                    .contains(&&s[split..])
            }),
            Self::Duration => value.as_str().is_some_and(|s| {
                if s.is_empty() || s.len() > 64 {
                    return false;
                }
                let mut rest = s;
                while !rest.is_empty() {
                    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
                    if digits == 0 || !rest[..digits].parse::<u64>().is_ok_and(|n| n > 0) {
                        return false;
                    }
                    rest = &rest[digits..];
                    if let Some(tail) = rest.strip_prefix("ms") {
                        rest = tail;
                    } else if rest.starts_with(['y', 'w', 'd', 'h', 'm', 's']) {
                        rest = &rest[1..];
                    } else {
                        return false;
                    }
                }
                true
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindingTransform {
    Sha256,
}

pub(crate) fn content_sha256(content: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(content.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    String,
    Integer,
    Number,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub kind: ParameterType,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub choices: Vec<Value>,
    #[serde(default)]
    pub minimum: Option<serde_json::Number>,
    #[serde(default)]
    pub maximum: Option<serde_json::Number>,
    #[serde(default)]
    pub max_length: Option<usize>,
    pub bindings: Vec<Binding>,
}
impl Parameter {
    fn validate(&self, value: &Value) -> Result<()> {
        ensure!(
            match self.kind {
                ParameterType::String =>
                    value.as_str().is_some_and(|v| !v.contains('\0')
                        && v.len() <= self.max_length.unwrap_or(4096).min(65536)),
                ParameterType::Integer => value.is_i64() || value.is_u64(),
                ParameterType::Number => value.is_number(),
                ParameterType::Boolean => value.is_boolean(),
            },
            "parameter_type_or_length_mismatch"
        );
        ensure!(
            self.choices.is_empty() || self.choices.contains(value),
            "parameter_not_in_choices"
        );
        if let Some(minimum) = &self.minimum {
            ensure!(
                value
                    .as_f64()
                    .is_some_and(|v| v >= minimum.as_f64().unwrap()),
                "parameter_below_minimum"
            );
        }
        if let Some(maximum) = &self.maximum {
            ensure!(
                value
                    .as_f64()
                    .is_some_and(|v| v <= maximum.as_f64().unwrap()),
                "parameter_above_maximum"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<super::inventory::Inventory>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bundle: BTreeMap<String, super::inventory::PackageFile>,
    pub uid: String,
    pub name: String,
    pub playbook: Playbook,
    pub targets: BTreeMap<String, TargetPreset>,
    #[serde(default)]
    pub fixed: crate::secrets::Variables,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: super::values::Values,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub immutable: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_values: BTreeMap<String, super::values::Values>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Parameter>,
    #[serde(default)]
    pub definitions: BTreeMap<String, String>,
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
    #[serde(default = "one")]
    pub batch_size: usize,
    #[serde(default = "crate::domain::default_download_parallel")]
    pub download_parallel: usize,
    #[serde(default)]
    pub reconcile_seconds: Option<u64>,
}
fn one() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workflow {
    pub uid: String,
    pub current: String,
    pub revision: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Actor {
    pub uid: String,
    pub name: String,
    pub fingerprint: String,
    pub credential_uid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub workflow_uid: String,
    pub workflow_revision: String,
    pub target: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
    pub request_id: String,
    pub plan_sha256: String,
    pub target_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Execution {
    pub request: Request,
    pub actor: Actor,
    pub parameters: BTreeMap<String, Value>,
    pub submitted_ms: u64,
    pub deployment_uid: Option<String>,
    #[serde(default)]
    pub system: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Registry {
    pub workflows: BTreeMap<String, Workflow>,
    pub revisions: BTreeMap<String, Arc<Revision>>,
    pub executions: BTreeMap<String, Execution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Register {
        workflow: Box<Revision>,
        expected_revision: u64,
    },
    Enable {
        uid: String,
        enabled: bool,
        expected_revision: u64,
    },
    Execute {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_key: Option<crate::model::outputs::PublicKey>,
        request: Box<Request>,
        actor: Actor,
        now_ms: u64,
    },
}

/// Restrict bindings to data fields, never interpreter, destination, identity or
/// ownership configuration. Plugins still validate the complete bound call.
pub(crate) fn bindable(operation: &Operation, path: &str) -> bool {
    if path
        .split('/')
        .skip(1)
        .any(|s| s.contains('~') || s.is_empty())
    {
        return false;
    }
    match operation {
        Operation::Exec {
            environment,
            fixed_environment,
            ..
        } => path.strip_prefix("/environment/").is_some_and(|name| {
            super::values::environment_allowed(name)
                && !fixed_environment.contains_key(name)
                && !matches!(
                    environment.get(name),
                    Some(crate::secrets::Value::Secret(_) | crate::secrets::Value::OutputSecret(_))
                )
        }),
        Operation::HttpCheck { .. } => [
            "/url",
            "/expected_status",
            "/body_sha256",
            "/timeout_seconds",
        ]
        .contains(&path),
        Operation::Reboot { .. } => path == "/requested",
        Operation::Service { .. } => ["/running", "/enabled"].contains(&path),
        Operation::Binary { .. } => path.starts_with("/artifact/") && !path.ends_with("/source"),
        Operation::Package { .. } => {
            path == "/version" || (path.starts_with("/artifact/") && !path.ends_with("/source"))
        }
        Operation::Plugin { .. } => {
            path.starts_with("/call/spec/")
                && !path.split('/').any(|s| {
                    [
                        "target",
                        "metadata",
                        "namespace",
                        "namespace_uid",
                        "node_group_uid",
                        "connection",
                        "credential_ref",
                        "owner",
                        "fence",
                        "resources",
                        "execution_profile",
                        "execution",
                        "script_sha256",
                        "kind",
                        "apiVersion",
                    ]
                    .contains(&s)
                })
        }
        Operation::File { .. } => path == "/content",
    }
}
pub fn safe_environment(name: &str) -> bool {
    !name.starts_with("LD_")
        && !name.starts_with("DYLD_")
        && !name.starts_with("BASH_FUNC_")
        && ![
            "PATH",
            "HOME",
            "SHELL",
            "BASH_ENV",
            "ENV",
            "SHELLOPTS",
            "BASHOPTS",
            "IFS",
            "CDPATH",
            "GLOBIGNORE",
            "PROMPT_COMMAND",
            "PS4",
            "GCONV_PATH",
            "LOCPATH",
            "NLSPATH",
            "TZDIR",
            "OPENSSL_CONF",
            "OPENSSL_MODULES",
            "PYTHONPATH",
            "PYTHONHOME",
            "PYTHONSTARTUP",
            "PERL5LIB",
            "PERL5OPT",
            "RUBYLIB",
            "RUBYOPT",
            "NODE_OPTIONS",
            "JAVA_TOOL_OPTIONS",
            "_JAVA_OPTIONS",
        ]
        .contains(&name)
}

impl Revision {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.playbook.api_version == "converyn/v1"
                && !self.playbook.name.trim().is_empty()
                && (1..=512).contains(&self.playbook.steps.len()),
            "invalid_workflow_playbook"
        );
        if let Some(inventory) = &self.inventory {
            inventory.validate()?;
        }
        super::inventory::validate_bundle(&self.bundle)?;
        for (path, entry) in &self.bundle {
            ensure!(
                super::inventory::valid_package_path(path)
                    && self.definitions.contains_key(&entry.sha256),
                "invalid_bundle_file"
            );
        }
        ensure!(
            valid_id(&self.uid)
                && self.uid != "register"
                && !self.name.trim().is_empty()
                && self.name.len() <= 256
                && self.targets.len() <= 64
                && self.parameters.len() <= 64
                && serde_json::to_vec(self)?.len() <= 1024 * 1024,
            "invalid_workflow"
        );
        crate::secrets::validate(&self.fixed)?;
        {
            let values = &self.values;
            super::values::validate(values)?;
            let defaults = serde_json::to_value(values)?;
            ensure!(
                self.immutable.iter().all(|path| !path.is_empty()
                    && path.split('.').all(|s| !s.is_empty())
                    && defaults.pointer(&super::values::pointer(path)).is_some()),
                "immutable_value_missing"
            );
            ensure!(
                self.node_values.len() <= crate::domain::MAX_NODES,
                "too_many_node_values"
            );
            for (node, values) in &self.node_values {
                ensure!(valid_id(node), "invalid_values_node");
                super::values::validate(values)?;
            }
        }
        ensure!(
            self.reconcile_seconds
                .is_none_or(|s| (30..=86400).contains(&s)),
            "invalid_reconcile_interval"
        );
        let has_hosts = self
            .playbook
            .steps
            .iter()
            .any(|s| s.operation.location() == converyn_plugin_api::Location::Node);
        for (uid, t) in &self.targets {
            ensure!(
                valid_id(uid)
                    && t.owner_uid.as_ref().is_none_or(|id| valid_id(id))
                    && t.node_group_uid
                        .iter()
                        .chain(t.external_target_uid.iter())
                        .all(|id| valid_id(id)),
                "invalid_target_preset"
            );
            ensure!(
                has_hosts == t.node_group_uid.is_some() && has_hosts == t.hosts.is_some(),
                "host_target_requires_node_group_and_hosts"
            );
            ensure!(
                has_hosts || t.external_target_uid.is_some(),
                "target_location_required"
            );
            if let Some(hosts) = &t.hosts {
                let inventory = self
                    .inventory
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("inventory_target_requires_inventory"))?;
                ensure!(
                    !inventory.select(hosts)?.is_empty(),
                    "empty_inventory_target"
                );
            }
        }
        for step in &self.playbook.steps {
            if let Operation::Exec {
                environment,
                fixed_environment,
                ..
            } = &step.operation
            {
                crate::secrets::validate(fixed_environment)?;
                for name in fixed_environment.keys() {
                    ensure!(
                        !environment.contains_key(name)
                            && !self.fixed.contains_key(name)
                            && !self.values.contains_key(name)
                            && !self.node_values.values().any(|v| v.contains_key(name))
                            && !self.inventory.as_ref().is_some_and(|i| i
                                .nodes
                                .iter()
                                .any(|h| h.values.contains_key(name))),
                        "fixed_step_environment_conflict:{name}"
                    );
                }
            }
            ensure!(step.node_order.is_empty(), "node_order_is_computed");
            if let Some(hosts) = &step.hosts {
                ensure!(
                    step.operation.location() == converyn_plugin_api::Location::Node,
                    "host_selector_requires_node_operation"
                );
                let inventory = self
                    .inventory
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("host_selector_requires_inventory"))?;
                ensure!(
                    step.allow_empty || !inventory.select(hosts)?.is_empty(),
                    "empty_host_selection"
                );
            } else {
                ensure!(!step.allow_empty, "allow_empty_requires_hosts");
            }
            crate::model::outputs::validate_specs(&step.outputs)?;
            for path in step.inputs.keys() {
                ensure!(
                    crate::model::outputs::bindable(&step.operation, path),
                    "output_input_field_not_allowed"
                );
                ensure!(
                    !self
                        .parameters
                        .values()
                        .any(|p| p.bindings.iter().any(|b| b.step_id == step.id
                            && (b.path == *path
                                || b.path.starts_with(&(path.clone() + "/"))
                                || path.starts_with(&(b.path.clone() + "/"))))),
                    "parameter_conflicts_with_output_input"
                );
                if let Some(name) = path.strip_prefix("/environment/") {
                    ensure!(
                        !self.fixed.contains_key(name)
                            && !self.values.contains_key(name)
                            && !self.node_values.values().any(|v| v.contains_key(name)),
                        "fixed_value_conflicts_with_output_input"
                    );
                }
            }
        }
        for (sha, content) in &self.definitions {
            use sha2::{Digest, Sha256};
            ensure!(
                content.len() <= 65536 && hex::encode(Sha256::digest(content.as_bytes())) == *sha,
                "invalid_workflow_definition"
            );
        }
        ensure!(
            self.playbook.steps.iter().all(|s| s
                .operation
                .definition_sha()
                .is_none_or(|sha| self.definitions.contains_key(sha))),
            "workflow_definition_missing"
        );
        let mut destinations: Vec<(String, String)> = Vec::new();
        for (name, parameter) in &self.parameters {
            ensure!(
                crate::secrets::name_valid(name)
                    && !self.fixed.contains_key(name)
                    && !parameter.bindings.is_empty()
                    && parameter.bindings.len() <= 64,
                "invalid_parameter_declaration"
            );
            if let Some(value) = &parameter.default {
                parameter.validate(value)?;
            }
            for choice in &parameter.choices {
                parameter.validate(choice)?;
            }
            for binding in &parameter.bindings {
                let step = self
                    .playbook
                    .steps
                    .iter()
                    .find(|s| s.id == binding.step_id)
                    .ok_or_else(|| anyhow::anyhow!("parameter_step_missing"))?;
                ensure!(
                    bindable(&step.operation, &binding.path),
                    "parameter_field_is_fixed"
                );
                ensure!(
                    binding.transform.is_none()
                        || (matches!(step.operation, Operation::Exec { .. })
                            && matches!(parameter.kind, ParameterType::String)),
                    "parameter_transform_not_allowed"
                );
                if matches!(step.operation, Operation::File { .. }) {
                    ensure!(
                        matches!(parameter.kind, ParameterType::String),
                        "configuration_must_be_text"
                    );
                }
                ensure!(
                    binding.allowed_json_paths.is_empty()
                        || (binding.path == "/content" && binding.allowed_json_paths.len() <= 64),
                    "configuration_paths_not_allowed"
                );
                ensure!(
                    binding
                        .scalar_rules
                        .keys()
                        .all(|path| binding.allowed_json_paths.contains(path)),
                    "configuration_rule_path_not_allowed"
                );
                if !binding.allowed_json_paths.is_empty() {
                    let original: Value = serde_json::from_str(
                        &self.definitions[step.operation.definition_sha().unwrap()],
                    )?;
                    ensure!(
                        binding
                            .allowed_json_paths
                            .iter()
                            .all(|path| path.starts_with('/')
                                && original
                                    .pointer(path)
                                    .is_some_and(|value| !value.is_array() && !value.is_object())),
                        "configuration_path_must_select_existing_scalar"
                    );
                }
                if let Some(name) = binding.path.strip_prefix("/environment/") {
                    ensure!(
                        !matches!(
                            self.fixed.get(name),
                            Some(
                                crate::secrets::Value::Secret(_)
                                    | crate::secrets::Value::OutputSecret(_)
                            )
                        ),
                        "secret_reference_is_fixed"
                    );
                }

                ensure!(
                    !destinations.iter().any(|(id, path)| id == &binding.step_id
                        && (path == &binding.path
                            || path.starts_with(&format!("{}/", binding.path))
                            || binding.path.starts_with(&format!("{path}/")))),
                    "overlapping_parameter_bindings"
                );
                destinations.push((binding.step_id.clone(), binding.path.clone()));
                if !matches!(
                    step.operation,
                    Operation::Exec { .. } | Operation::File { .. }
                ) {
                    ensure!(
                        serde_json::to_value(&step.operation)?
                            .pointer(&binding.path)
                            .is_some_and(|v| v.is_string()
                                || v.is_number()
                                || v.is_boolean()
                                || v.is_null()),
                        "parameter_must_bind_existing_scalar"
                    );
                }
            }
        }
        Ok(())
    }
}

impl State {
    pub fn workflow_plan(
        &self,
        uid: &str,
        revision: &str,
        target: &str,
        overrides: &BTreeMap<String, Value>,
        contracts: &dyn converyn_plugin_api::Contracts,
    ) -> Result<(Plan, BTreeMap<String, Value>, String)> {
        let w = self
            .workflows
            .workflows
            .get(uid)
            .ok_or_else(|| anyhow::anyhow!("workflow_not_found"))?;
        ensure!(
            w.enabled && w.current == revision,
            "workflow_disabled_or_revision_changed"
        );
        let source = self
            .workflows
            .revisions
            .get(revision)
            .ok_or_else(|| anyhow::anyhow!("workflow_revision_missing"))?;
        for step in &source.playbook.steps {
            for path in step.inputs.keys() {
                if let Some(name) = path.strip_prefix("/environment/") {
                    ensure!(!overrides.contains_key(name), "output_input_is_fixed");
                }
            }
        }
        let preset = source
            .targets
            .get(target)
            .ok_or_else(|| anyhow::anyhow!("target_not_allowed"))?;
        let hosts = if let Some(group_uid) = &preset.node_group_uid {
            let group = self
                .node_groups
                .get(group_uid)
                .ok_or_else(|| anyhow::anyhow!("node_group_not_found:{group_uid}"))?;
            let inventory = source.inventory.as_ref().unwrap();
            // Every declared identity must be enrolled, even if this target selects a subset.
            for host in &inventory.nodes {
                ensure!(
                    self.node_enrolled(&host.uid),
                    "inventory_node_not_enrolled:{}",
                    host.uid
                );
            }
            let selected = inventory.select(preset.hosts.as_deref().unwrap())?;
            let outside: Vec<_> = selected
                .iter()
                .filter(|id| !group.node_uids.contains(*id))
                .collect();
            ensure!(
                outside.is_empty(),
                "node_group_scope_invalid:group={group_uid}:target={target}:nodes={outside:?}"
            );
            Some(crate::domain::HostsSnapshot {
                node_group_uid: group_uid.clone(),
                revision: group.revision,
                node_uids: selected,
            })
        } else {
            None
        };
        let nodes: Vec<_> = hosts.iter().flat_map(|h| h.node_uids.iter()).collect();
        // Secret references stay outside the public values document.
        let resolved_values;
        let mut workflow_values = BTreeMap::new();
        {
            let defaults = &source.values;
            let overrides: super::values::Values = overrides
                .iter()
                .filter(|(name, _)| !source.parameters.contains_key(*name))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            super::values::validate(&overrides)?;
            let globals =
                super::values::resolve(defaults, &BTreeMap::new(), &overrides, &source.immutable)?;
            resolved_values = globals;
            for node in &nodes {
                let mut local: super::values::Values =
                    BTreeMap::<String, crate::secrets::Value>::new()
                        .iter()
                        .filter_map(|(name, value)| match value {
                            crate::secrets::Value::Literal(value)
                                if defaults.contains_key(name) =>
                            {
                                let value = if defaults[name].is_string() {
                                    Value::String(value.clone())
                                } else {
                                    serde_json::from_str(value)
                                        .unwrap_or_else(|_| Value::String(value.clone()))
                                };
                                Some((name.clone(), value))
                            }
                            _ => None,
                        })
                        .collect();
                if let Some(host) = source
                    .inventory
                    .as_ref()
                    .and_then(|i| i.nodes.iter().find(|h| &h.uid == *node))
                {
                    super::values::overlay(&mut local, &host.values);
                }
                if let Some(values) = source.node_values.get(*node) {
                    super::values::overlay(&mut local, values);
                }
                let values =
                    super::values::resolve(defaults, &local, &overrides, &source.immutable)?;
                let environment = super::values::environment(&values)?;
                for name in environment.keys() {
                    ensure!(!source.fixed.contains_key(name) && !source.parameters.contains_key(name)
                        && !source.parameters.values().any(|p| p.bindings.iter().any(|b| b.path == format!("/environment/{name}")))
                        && !source.playbook.steps.iter().any(|step| matches!(&step.operation, Operation::Exec { environment, fixed_environment, .. } if fixed_environment.contains_key(name) || matches!(environment.get(name), Some(crate::secrets::Value::Secret(_) | crate::secrets::Value::OutputSecret(_))))),
                        "workflow_value_conflicts_with_fixed_binding:{name}");
                }
                workflow_values.insert((*node).clone(), environment);
            }
        }
        let external = preset
            .external_target_uid
            .as_ref()
            .map(|id| {
                self.targets
                    .get(id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("target_not_found"))
            })
            .transpose()?;
        let target_sha = digest(&(&hosts, &external));
        let mut input = PlanInput {
            inventory: source.inventory.clone(),
            bundle: source.bundle.clone(),
            target: external,
            project: Some("default".into()),
            hosts,
            playbook: source.playbook.clone(),
            batch_size: source.batch_size,
            download_parallel: source.download_parallel,
            owner_uid: preset
                .owner_uid
                .clone()
                .unwrap_or_else(|| format!("workflow-{}", &digest(&(uid, target))[..32])),
            parameters: source.fixed.clone(),
            workflow_values,
        };
        for step in &mut input.playbook.steps {
            if let Some(inventory) = &source.inventory {
                if step.operation.location() == converyn_plugin_api::Location::Node {
                    let selected = inventory.select(step.hosts.as_deref().unwrap_or("all"))?;
                    step.node_order = selected
                        .into_iter()
                        .filter(|id| {
                            input
                                .hosts
                                .as_ref()
                                .is_some_and(|h| h.node_uids.contains(id))
                        })
                        .collect();
                    ensure!(
                        step.allow_empty || !step.node_order.is_empty(),
                        "empty_host_selection"
                    );
                }
                for reference in step.inputs.values_mut() {
                    if let Some(selector) = reference.node.as_ref().filter(|n| n.contains('[')) {
                        let ids = inventory.select(selector)?;
                        ensure!(ids.len() == 1, "output_producer_must_be_single_node");
                        reference.node = Some(ids[0].clone());
                    }
                }
            }
            if step.operation.location() == converyn_plugin_api::Location::Controller {
                continue;
            }
        }
        let mut parameters = BTreeMap::new();
        parameters.extend(resolved_values);
        // Fixed environment layers are merged before explicit parameter bindings.
        for (name, p) in &source.parameters {
            let value = overrides
                .get(name)
                .or(p.default.as_ref())
                .ok_or_else(|| anyhow::anyhow!("required_parameter_missing:{name}"))?;
            p.validate(value)?;
            parameters.insert(name.clone(), value.clone());
            for binding in &p.bindings {
                let value = if binding.transform == Some(BindingTransform::Sha256) {
                    Value::String(content_sha256(value.as_str().unwrap()))
                } else {
                    value.clone()
                };
                let step = input
                    .playbook
                    .steps
                    .iter_mut()
                    .find(|s| s.id == binding.step_id)
                    .unwrap();
                if let Operation::Exec { environment, .. } = &mut step.operation {
                    environment.insert(
                        binding.path["/environment/".len()..].to_string(),
                        value
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| value.to_string())
                            .into(),
                    );
                } else if let Operation::File { path, sha256, .. } = &mut step.operation {
                    // The declaration fixes destination, permissions and ownership;
                    // only the bytes of this Run receive a new content address.
                    let content = value.as_str().unwrap();
                    if !binding.allowed_json_paths.is_empty() {
                        let actual: Value = serde_json::from_str(content)?;
                        let mut permitted: Value =
                            serde_json::from_str(&source.definitions[sha256.as_str()])?;
                        for path in &binding.allowed_json_paths {
                            let changed = actual
                                .pointer(path)
                                .ok_or_else(|| anyhow::anyhow!("configuration_field_missing"))?;
                            ensure!(
                                !changed.is_array() && !changed.is_object(),
                                "configuration_field_must_be_scalar"
                            );
                            // Helm charts may evaluate strings with tpl(). A
                            // declared scalar is data, never a template program.
                            ensure!(
                                changed.as_str().is_none_or(|s| !s.contains("{{")
                                    && !s.contains("}}")
                                    && !s.chars().any(char::is_control)),
                                "configuration_template_not_allowed"
                            );
                            if let Some(rule) = binding.scalar_rules.get(path) {
                                ensure!(
                                    rule.accepts(changed),
                                    "configuration_scalar_out_of_range_or_format"
                                );
                            }
                            *permitted.pointer_mut(path).unwrap() = changed.clone();
                        }
                        ensure!(permitted == actual, "configuration_field_is_fixed");
                    }
                    if path.ends_with(".json") {
                        serde_json::from_str::<Value>(content)
                            .map_err(|_| anyhow::anyhow!("invalid_configuration_json"))?;
                    }
                    *sha256 = content_sha256(content);
                } else {
                    let mut operation = serde_json::to_value(&step.operation)?;
                    *operation
                        .pointer_mut(&binding.path)
                        .ok_or_else(|| anyhow::anyhow!("parameter_field_missing"))? = value.clone();
                    step.operation = serde_json::from_value(operation)?;
                }
            }
        }
        ensure!(
            serde_json::to_vec(&parameters)?.len() <= 262144,
            "parameter_snapshot_exceeds_limit"
        );
        let plan = input.compile_with(contracts)?;
        if source.reconcile_seconds.is_some() {
            ensure!(
                source
                    .playbook
                    .steps
                    .iter()
                    .all(|s| s.inputs.is_empty() && s.outputs.is_empty()),
                "Run dataflow is execution-scoped; declare stable reconciliation inputs explicitly"
            );
            crate::drift::plan(&plan).map_err(|e| anyhow::anyhow!(e))?;
        }
        Ok((plan, parameters, target_sha))
    }

    pub fn workflow_change(
        &mut self,
        change: Change,
        contracts: &dyn converyn_plugin_api::Contracts,
    ) -> Result<Reply> {
        match change {
            Change::Register {
                workflow,
                expected_revision,
            } => {
                workflow.validate()?;
                // Even an unchanged definition must be checked against current platform scope.
                for (name, preset) in &workflow.targets {
                    if let Some(uid) = &preset.node_group_uid {
                        let group = self
                            .node_groups
                            .get(uid)
                            .ok_or_else(|| anyhow::anyhow!("node_group_not_found:{uid}"))?;
                        let inventory = workflow.inventory.as_ref().unwrap();
                        for node in &inventory.nodes {
                            ensure!(
                                self.node_enrolled(&node.uid),
                                "inventory_node_not_enrolled:{}",
                                node.uid
                            );
                        }
                        let outside: Vec<_> = inventory
                            .select(preset.hosts.as_deref().unwrap())?
                            .into_iter()
                            .filter(|id| !group.node_uids.contains(id))
                            .collect();
                        ensure!(
                            outside.is_empty(),
                            "node_group_scope_invalid:group={uid}:target={name}:nodes={outside:?}"
                        );
                    }
                }
                let sha = digest(&workflow);
                let already_stored = self.workflows.revisions.contains_key(&sha);
                let old = self.workflows.workflows.get(&workflow.uid);
                if old.is_some_and(|w| w.current == sha) {
                    return Ok(Reply {
                        ok: true,
                        code: "workflow_unchanged".into(),
                        run_id: None,
                        revision: old.map(|w| w.revision),
                    });
                }
                ensure!(
                    old.map_or(0, |w| w.revision) == expected_revision,
                    "workflow_revision_conflict"
                );
                ensure!(
                    self.workflows.workflows.len() < 1000 || old.is_some(),
                    "workflow_limit"
                );
                ensure!(
                    self.workflows
                        .revisions
                        .values()
                        .map(|r| serde_json::to_vec(r).unwrap().len())
                        .sum::<usize>()
                        + if self.workflows.revisions.contains_key(&sha) {
                            0
                        } else {
                            serde_json::to_vec(&workflow)?.len()
                        }
                        <= 32 * 1024 * 1024,
                    "workflow_content_limit"
                );
                let uid = workflow.uid.clone();
                let enabled = old.is_none_or(|w| w.enabled);
                let previous = self.workflows.workflows.insert(
                    uid.clone(),
                    Workflow {
                        uid: uid.clone(),
                        current: sha.clone(),
                        revision: expected_revision + 1,
                        enabled: true,
                    },
                );
                self.workflows
                    .revisions
                    .insert(sha.clone(), Arc::from(*workflow));
                let source = self.workflows.revisions[&sha].clone();
                // Registration validates every preset before publishing any change.
                for target in source.targets.keys() {
                    let mut samples = BTreeMap::new();
                    for (name, p) in &source.parameters {
                        if p.default.is_none() {
                            let base = p
                                .bindings
                                .iter()
                                .filter(|b| b.path == "/content")
                                .chain(p.bindings.iter().filter(|b| b.path != "/content"))
                                .filter_map(|binding| {
                                    source
                                        .playbook
                                        .steps
                                        .iter()
                                        .find(|step| step.id == binding.step_id)
                                        .and_then(|step| {
                                            if binding.path == "/content" {
                                                return step.operation.definition_sha().and_then(|sha| source.definitions.get(sha))
                                                    .map(|content| serde_json::json!({"content":content}));
                                            }
                                            serde_json::to_value(&step.operation).ok()
                                        })
                                        .and_then(|operation| {
                                            operation.pointer(&binding.path).cloned()
                                        })
                                })
                                .find(|value| p.validate(value).is_ok());
                            let sample = p.choices.first().cloned().or(base).unwrap_or_else(|| {
                                match p.kind {
                                    ParameterType::String => Value::String(
                                        if p.bindings.iter().any(|b| b.path.ends_with("sha256")) {
                                            "0".repeat(64)
                                        } else {
                                            "example"
                                                .chars()
                                                .take(p.max_length.unwrap_or(7))
                                                .collect()
                                        },
                                    ),
                                    ParameterType::Boolean => Value::Bool(false),
                                    ParameterType::Integer => Value::Number(
                                        p.minimum
                                            .as_ref()
                                            .map(|v| (v.as_f64().unwrap().ceil() as i64).into())
                                            .or_else(|| {
                                                p.maximum.as_ref().map(|v| {
                                                    (v.as_f64().unwrap().floor() as i64).into()
                                                })
                                            })
                                            .unwrap_or_else(|| 0.into()),
                                    ),
                                    ParameterType::Number => Value::Number(
                                        p.minimum
                                            .clone()
                                            .or_else(|| p.maximum.clone())
                                            .unwrap_or_else(|| 0.into()),
                                    ),
                                }
                            });
                            samples.insert(name.clone(), sample);
                        }
                    }
                    if let Err(error) = self.workflow_plan(&uid, &sha, target, &samples, contracts)
                    {
                        self.workflows.workflows.remove(&uid);
                        if let Some(old) = previous {
                            self.workflows.workflows.insert(uid, old);
                        }
                        if !already_stored {
                            self.workflows.revisions.remove(&sha);
                        }
                        return Err(error);
                    }
                }
                self.workflows.workflows.get_mut(&uid).unwrap().enabled = enabled;
                Ok(Reply {
                    ok: true,
                    code: "workflow_registered".into(),
                    run_id: None,
                    revision: Some(expected_revision + 1),
                })
            }
            Change::Enable {
                uid,
                enabled,
                expected_revision,
            } => {
                let w = self
                    .workflows
                    .workflows
                    .get_mut(&uid)
                    .ok_or_else(|| anyhow::anyhow!("workflow_not_found"))?;
                if w.enabled == enabled {
                    return Ok(Reply {
                        ok: true,
                        code: "workflow_unchanged".into(),
                        run_id: None,
                        revision: Some(w.revision),
                    });
                }
                ensure!(
                    w.revision == expected_revision,
                    "workflow_revision_conflict"
                );
                w.enabled = enabled;
                w.revision += 1;
                Ok(Reply {
                    ok: true,
                    code: "workflow_updated".into(),
                    run_id: None,
                    revision: Some(w.revision),
                })
            }
            Change::Execute {
                output_key,
                request,
                actor,
                now_ms,
            } => {
                let request = *request;
                ensure!(
                    valid_id(&request.request_id) && request.request_id.len() <= 124,
                    "invalid_request_id"
                );
                let id = format!("run-{}", request.request_id);
                ensure!(
                    !self.retirement_proofs.contains_key(&format!("run/{id}")),
                    "run_history_cleared"
                );
                if let Some(old) = self.workflows.executions.get(&id) {
                    ensure!(
                        old.request == request && old.actor.uid == actor.uid,
                        "run_idempotency_conflict"
                    );
                    return Ok(Reply {
                        ok: true,
                        code: "existing_run".into(),
                        run_id: Some(id),
                        revision: None,
                    });
                }
                let (plan, parameters, target_sha) = self.workflow_plan(
                    &request.workflow_uid,
                    &request.workflow_revision,
                    &request.target,
                    &request.parameters,
                    contracts,
                )?;
                ensure!(
                    plan.sha256 == request.plan_sha256 && target_sha == request.target_sha256,
                    "preview_stale_replan_required"
                );
                let source = self.workflows.revisions[&request.workflow_revision].clone();
                let deployment_uid = source
                    .reconcile_seconds
                    .map(|_| plan.input.owner_uid.clone());
                if let Some(uid) = &deployment_uid {
                    ensure!(
                        self.deployments
                            .get(uid)
                            .is_none_or(|d| d.active_run.is_none())
                            && self.drift.get(uid).is_none_or(|p| p.active_run.is_none()),
                        "desired_state_busy"
                    );
                }
                for (sha, content) in &source.definitions {
                    let reply = self.apply_with(
                        Command::PutScript {
                            sha256: sha.clone(),
                            content: content.clone(),
                        },
                        contracts,
                    );
                    ensure!(reply.ok, "definition_admission_failed");
                }
                for (name, parameter) in &source.parameters {
                    if parameter
                        .bindings
                        .iter()
                        .any(|binding| binding.path == "/content")
                    {
                        let content = parameters[name].as_str().unwrap();
                        let reply = self.apply_with(
                            Command::PutScript {
                                sha256: content_sha256(content),
                                content: content.to_owned(),
                            },
                            contracts,
                        );
                        ensure!(reply.ok, "configuration_definition_admission_failed");
                    }
                }
                let reply = self.apply_with(
                    Command::CreateRun {
                        output_key: output_key.clone(),
                        run_id: id.clone(),
                        idempotency_key: format!("workflow/{}", request.request_id),
                        plan: Box::new(plan.clone()),
                    },
                    contracts,
                );
                ensure!(reply.ok, "run_admission_failed:{}", reply.code);
                if let Some(uid) = &deployment_uid {
                    let generation = self.deployments.get(uid).map_or(1, |d| d.generation + 1);
                    self.deployments.insert(
                        uid.clone(),
                        crate::machine::Deployment {
                            uid: uid.clone(),
                            generation,
                            applied_generation: None,
                            last_started_generation: generation,
                            plan: Arc::new(plan),
                            active_run: Some(id.clone()),
                            last_run: Some(id.clone()),
                            condition: "progressing".into(),
                        },
                    );
                    self.configure_drift(uid, source.reconcile_seconds)
                        .map_err(|e| anyhow::anyhow!(e))?;
                }
                self.workflows.executions.insert(
                    id,
                    Execution {
                        request,
                        actor,
                        parameters,
                        submitted_ms: now_ms,
                        deployment_uid,
                        system: false,
                    },
                );
                Ok(reply)
            }
        }
    }
}

#[cfg(test)]
mod scalar_rule_tests {
    use super::*;
    #[test]
    fn configuration_quantities_and_durations_are_data_not_helm_programs() {
        for value in ["50m", "128Mi", "10Gi", "0.5", "1"] {
            assert!(ScalarRule::Quantity.accepts(&Value::String(value.into())));
        }
        for value in [
            "",
            "0",
            "-1Gi",
            "{{lookup secret}}",
            "1Gi\n",
            "NaN",
            "1;cat /etc/passwd",
        ] {
            assert!(!ScalarRule::Quantity.accepts(&Value::String(value.into())));
        }
        for value in ["2d", "1h30m", "500ms"] {
            assert!(ScalarRule::Duration.accepts(&Value::String(value.into())));
        }
        for value in ["0s", "2", "-1h", "{{fail}}", "1d\n", "forever"] {
            assert!(!ScalarRule::Duration.accepts(&Value::String(value.into())));
        }
    }
}
