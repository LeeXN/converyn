//! 固定目标、参数与步骤的计划编译；不访问主机或场景插件。
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_NODES: usize = 300;
pub const MAX_ARTIFACT_SIZE: u64 = 10 * 1024 * 1024 * 1024;
pub const MAX_PLAN_BYTES: usize = 1024 * 1024;

pub fn digest(value: &impl Serialize) -> String {
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, value).expect("serializable protocol value");
    hex::encode(writer.0.finalize())
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && !id.contains("..")
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

/// Platform membership is an unordered authorization boundary, independent of workflow topology.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeGroup {
    pub uid: String,
    pub name: String,
    pub revision: u64,
    pub node_uids: BTreeSet<String>,
}
impl NodeGroup {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.uid)
                && !self.name.trim().is_empty()
                && self.name.len() <= 256
                && self.revision > 0
                && self.node_uids.len() <= MAX_NODES
                && self.node_uids.iter().all(|id| valid_id(id)),
            "invalid_node_group"
        );
        Ok(())
    }
}
/// Frozen workflow selection; membership changes never rewrite this ordered list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostsSnapshot {
    pub node_group_uid: String,
    pub revision: u64,
    pub node_uids: Vec<String>,
}
impl HostsSnapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.node_group_uid)
                && self.revision > 0
                && (1..=MAX_NODES).contains(&self.node_uids.len())
                && self.node_uids.iter().all(|id| valid_id(id))
                && self.node_uids.iter().collect::<BTreeSet<_>>().len() == self.node_uids.len(),
            "invalid_hosts_snapshot"
        );
        Ok(())
    }
}
/// Internal executor identity. It carries no platform roles or host selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub uid: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepare,
    Rollout,
    Verify,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    OncePerRun,
    OncePerBatch,
    PerNode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Plugin {
        call: Box<converyn_plugin_api::Call>,
    },

    HttpCheck {
        url: String,
        expected_status: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body_sha256: Option<String>,
        timeout_seconds: u32,
    },

    Package {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        install_only: bool,
        manager: crate::package::Manager,
        name: String,
        version: String,
        artifact: crate::source::ArtifactRef,
        timeout_seconds: u32,
    },
    Binary {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version_directory: Option<String>,
        path: String,
        artifact: crate::source::ArtifactRef,
        mode: u32,
    },
    File {
        path: String,
        sha256: String,
        mode: u32,
    },
    Service {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        executable_sha256: Option<String>,
        name: String,
        running: bool,
        enabled: bool,
    },
    Exec {
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        fixed_environment: crate::secrets::Variables,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        resources: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution_profile: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        execution: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<converyn_plugin_api::ScriptOrigin>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        environment: crate::secrets::Variables,
        script_sha256: String,
        timeout_seconds: u32,
    },
    Reboot {
        #[serde(default = "enabled")]
        requested: bool,
        recovery_timeout_seconds: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_kernel: Option<String>,
    },
}

pub(crate) fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Operation {
    pub fn location(&self) -> converyn_plugin_api::Location {
        match self {
            Self::Plugin { call } => call.location,
            _ => converyn_plugin_api::Location::Node,
        }
    }

    pub fn artifact(&self) -> Option<&crate::source::ArtifactRef> {
        match self {
            Self::Binary { artifact, .. } | Self::Package { artifact, .. } => Some(artifact),
            _ => None,
        }
    }
    pub fn definition_sha(&self) -> Option<&str> {
        match self {
            Self::Exec { script_sha256, .. } => Some(script_sha256),
            Self::File { sha256, .. } => Some(sha256),
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Plugin { call } => call.validate()?,

            Self::HttpCheck {
                url,
                expected_status,
                body_sha256,
                timeout_seconds,
            } => {
                crate::health::validate_url(url)?;
                ensure!(
                    (100..=599).contains(expected_status)
                        && (1..=3600).contains(timeout_seconds)
                        && body_sha256.as_ref().is_none_or(|s| valid_sha(s)),
                    "invalid health expectation or timeout"
                );
            }

            Self::Package {
                install_only,
                manager,
                name,
                version,
                artifact,
                timeout_seconds,
            } => {
                artifact.validate()?;
                ensure!(
                    !install_only || matches!(manager, crate::package::Manager::Rpm),
                    "install_only is supported for RPM multiversion packages"
                );
                ensure!(
                    !name.is_empty()
                        && name.len() <= 128
                        && name.as_bytes()[0].is_ascii_alphanumeric()
                        && name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-+._".contains(&b)),
                    "invalid package name"
                );
                ensure!(
                    !version.is_empty()
                        && version.len() <= 128
                        && version.bytes().all(|b| b.is_ascii_graphic())
                        && (1..=86400).contains(timeout_seconds),
                    "invalid exact package version or timeout"
                );
            }
            Self::Binary {
                path,
                artifact,
                mode,
                version_directory,
            } => {
                if let Some(directory) = version_directory {
                    Self::File {
                        path: format!("{directory}/{}-{:o}", artifact.sha256, mode),
                        sha256: artifact.sha256.clone(),
                        mode: *mode,
                    }
                    .validate()?;
                    ensure!(
                        !directory.ends_with('/')
                            && directory != path
                            && path != &format!("{directory}/{}-{:o}", artifact.sha256, mode),
                        "invalid binary version directory"
                    );
                }
                artifact.validate()?;
                Self::File {
                    path: path.clone(),
                    sha256: artifact.sha256.clone(),
                    mode: *mode,
                }
                .validate()?;
            }
            Self::File { path, sha256, mode } => {
                ensure!(
                    path.starts_with('/')
                        && path != "/"
                        && path.len() < 4096
                        && !path.contains('\0'),
                    "file path must be absolute and non-root"
                );
                ensure!(
                    !path
                        .split('/')
                        .skip(1)
                        .any(|p| p.is_empty() || p == "." || p == ".."),
                    "file path must be normalized"
                );
                ensure!(
                    valid_sha(sha256) && *mode <= 0o777,
                    "invalid file digest or mode"
                );
            }
            Self::Service {
                name,
                running,
                executable_sha256,
                ..
            } => ensure!(
                valid_id(name)
                    && name.ends_with(".service")
                    && executable_sha256
                        .as_ref()
                        .is_none_or(|sha| *running && valid_sha(sha)),
                "invalid service name or executable digest"
            ),
            Self::Exec {
                resources,
                execution_profile,
                execution,
                origin,
                script_sha256,
                timeout_seconds,
                environment,
                fixed_environment,
            } => {
                ensure!(
                    resources.len() <= 128
                        && resources.windows(2).all(|w| w[0] < w[1])
                        && resources.iter().all(|key| !key.is_empty()
                            && key.len() <= 4096
                            && !key.starts_with('/')
                            && !key.contains(['\0', '\n'])
                            && !key
                                .split('/')
                                .any(|s| s.is_empty() || s == ".." || s == ".")),
                    "invalid script resources"
                );
                ensure!(
                    execution_profile.as_ref().is_none_or(|p| valid_id(p)),
                    "invalid execution profile reference"
                );
                ensure!(
                    execution
                        .as_deref()
                        .is_none_or(|v| matches!(v, "user" | "host")),
                    "execution must be user or host"
                );
                if let Some(origin) = origin {
                    ensure!(
                        valid_id(&origin.plugin.id)
                            && valid_id(&origin.plugin.version)
                            && valid_sha(&origin.plugin.sha256)
                            && origin.plugin.contract == 1
                            && valid_id(&origin.action),
                        "invalid script plugin origin"
                    );
                }
                crate::secrets::validate(environment)?;
                crate::secrets::validate(fixed_environment)?;
                ensure!(
                    valid_sha(script_sha256) && (1..=86400).contains(timeout_seconds),
                    "invalid script digest or timeout"
                );
            }
            Self::Reboot {
                requested: _,
                recovery_timeout_seconds,
                expected_kernel,
            } => ensure!(
                (1..=86400).contains(recovery_timeout_seconds)
                    && expected_kernel.as_ref().is_none_or(|k| valid_id(k)),
                "invalid reboot timeout"
            ),
        }
        Ok(())
    }

    pub fn resource_keys(&self, node: &str) -> Vec<String> {
        let mut keys = self.resource_key(node).into_iter().collect::<Vec<_>>();
        if let Self::Exec { resources, .. } = self {
            keys.extend(resources.iter().map(|key| format!("{node}/{key}")));
        }
        if let Self::Plugin { call } = self {
            keys.extend(call.resources.iter().map(|key| {
                if call.location == converyn_plugin_api::Location::Controller {
                    key.clone()
                } else {
                    format!("{node}/{key}")
                }
            }));
        }
        if let Self::Binary {
            version_directory: Some(directory),
            artifact,
            mode,
            ..
        } = self
        {
            keys.push(format!(
                "{node}/file{directory}/{}-{:o}",
                artifact.sha256, mode
            ));
        }
        keys
    }
    pub fn resource_key(&self, node: &str) -> Option<String> {
        match self {
            Self::Package { manager, name, .. } => {
                Some(format!("{node}/package/{manager:?}/{name}"))
            }
            Self::File { path, .. } | Self::Binary { path, .. } => {
                Some(format!("{node}/file{path}"))
            }
            Self::Service { name, .. } => Some(format!("{node}/service/{name}")),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_empty: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_order: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: super::outputs::Specifications,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: super::outputs::Bindings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<usize>,
    pub id: String,
    pub phase: Phase,
    pub scope: Scope,
    pub operation: Operation,
}

impl Step {
    pub fn selects(&self, node: &Node) -> bool {
        self.node_order.is_empty() && self.hosts.is_none() || self.node_order.contains(&node.uid)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Playbook {
    pub api_version: String,
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<crate::orchestration::inventory::Inventory>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bundle: BTreeMap<String, crate::orchestration::inventory::PackageFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<super::target::ExternalTarget>,
    #[serde(
        default = "default_download_parallel",
        skip_serializing_if = "default_download_limit"
    )]
    pub download_parallel: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hosts: Option<HostsSnapshot>,
    pub playbook: Playbook,
    pub batch_size: usize,
    /// Managed ownership persists across Runs. Ephemeral maintenance ownership is not part of P0.
    pub owner_uid: String,
    #[serde(default)]
    pub parameters: crate::secrets::Variables,
    /// Per-node workflow globals, already resolved against this Run's overrides.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub workflow_values: BTreeMap<String, crate::secrets::Variables>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Work {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: super::outputs::Specifications,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: super::outputs::Bindings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_sha256: Option<String>,
    pub id: String,
    pub step_id: String,
    pub node_uid: String,
    pub phase: Phase,
    pub batch: Option<u32>,
    pub wave: u32,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub format: u32,
    pub input: PlanInput,
    pub sha256: String,
    pub work: Vec<Work>,
}

fn enabled() -> bool {
    true
}

pub fn default_download_parallel() -> usize {
    16
}
pub fn default_download_limit(value: &usize) -> bool {
    *value == 16
}
impl PlanInput {
    pub fn host_nodes(&self) -> Vec<Node> {
        self.hosts
            .iter()
            .flat_map(|h| h.node_uids.iter())
            .map(|uid| Node { uid: uid.clone() })
            .collect()
    }
    /// The first rollout selector establishes batch order; later selectors must
    /// agree across batches so no host silently changes its workflow position.
    pub(crate) fn rollout_nodes(&self) -> Vec<Node> {
        let mut nodes = Vec::new();
        for step in self.playbook.steps.iter().filter(|s| {
            s.phase == Phase::Rollout
                && s.scope == Scope::PerNode
                && s.operation.location() == converyn_plugin_api::Location::Node
        }) {
            for uid in &step.node_order {
                if !nodes.iter().any(|node: &Node| &node.uid == uid) {
                    nodes.push(Node { uid: uid.clone() });
                }
            }
        }
        nodes
    }
    pub fn location_uid(&self) -> &str {
        self.hosts
            .as_ref()
            .map(|h| h.node_group_uid.as_str())
            .or_else(|| self.target.as_ref().map(|t| t.uid.as_str()))
            .unwrap_or("")
    }

    /// 使用同一组纯插件契约冻结外部目标；不执行插件的任何外部操作。
    pub fn compile_with(mut self, contracts: &dyn converyn_plugin_api::Contracts) -> Result<Plan> {
        if let Some(inventory) = &self.inventory {
            inventory.validate()?;
            ensure!(
                self.host_nodes()
                    .iter()
                    .all(|n| inventory.nodes.iter().any(|h| h.uid == n.uid)),
                "inventory_snapshot_mismatch"
            );
        }
        crate::orchestration::inventory::validate_bundle(&self.bundle)?;
        for (path, file) in &self.bundle {
            ensure!(
                crate::orchestration::inventory::valid_package_path(path)
                    && valid_sha(&file.sha256),
                "invalid_bundle_file"
            );
        }
        for step in &mut self.playbook.steps {
            if let Operation::Plugin { call } = &mut step.operation {
                if call.location == converyn_plugin_api::Location::Controller
                    && let Some(target) = &self.target
                {
                    **call = contracts.bind_target(call, &target.spec)?;
                } else {
                    contracts.validate_call(call)?;
                }
            }
        }
        self.compile()
    }
    pub fn compile(mut self) -> Result<Plan> {
        let private_reference = |values: &crate::secrets::Variables| {
            values
                .values()
                .any(|v| matches!(v, crate::secrets::Value::OutputSecret(_)))
        };
        ensure!(
            !private_reference(&self.parameters)
                && self.workflow_values.values().all(|v| !private_reference(v)),
            "Run output secret references are internal"
        );
        for step in &self.playbook.steps {
            for path in step.inputs.keys() {
                ensure!(
                    super::outputs::bindable(&step.operation, path),
                    "output_input_field_not_allowed"
                );
            }
            if let Operation::Exec {
                environment,
                fixed_environment,
                ..
            } = &step.operation
            {
                ensure!(
                    !private_reference(environment) && !private_reference(fixed_environment),
                    "Run output secret references are internal"
                );
            }
        }
        let has_node_steps = self
            .playbook
            .steps
            .iter()
            .any(|s| s.operation.location() == converyn_plugin_api::Location::Node);
        let nodes = self.host_nodes();
        let controller_node = self.target.as_ref().map(|t| t.executor());
        if let Some(target) = &self.target {
            target.validate()?;
            ensure!(
                self.project.as_deref() == Some(&target.project),
                "external target project mismatch"
            );
            ensure!(self.playbook.steps.iter().all(|s|!matches!(&s.operation,Operation::Plugin{call} if call.location==converyn_plugin_api::Location::Controller&&call.plugin!=target.plugin)),"external target plugin mismatch");
        } else {
            ensure!(
                !self
                    .playbook
                    .steps
                    .iter()
                    .any(|s| s.operation.location() == converyn_plugin_api::Location::Controller),
                "controller plugin requires an external target"
            );
        }
        ensure!(
            has_node_steps == self.hosts.is_some(),
            "host_operations_require_hosts_snapshot"
        );
        if let Some(hosts) = &self.hosts {
            hosts.validate()?;
            let inventory = self
                .inventory
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("hosts_require_inventory"))?;
            inventory.validate()?;
            ensure!(
                hosts
                    .node_uids
                    .iter()
                    .all(|id| inventory.nodes.iter().any(|h| &h.uid == id)),
                "inventory_snapshot_mismatch"
            );
        }
        // Selection is resolved from the immutable inventory, including offline compilation.
        for step in &mut self.playbook.steps {
            if step.operation.location() == converyn_plugin_api::Location::Node {
                let selected = self
                    .inventory
                    .as_ref()
                    .unwrap()
                    .select(step.hosts.get_or_insert_with(|| "all".into()))?;
                step.node_order = selected
                    .into_iter()
                    .filter(|id| nodes.iter().any(|n| &n.uid == id))
                    .collect();
                ensure!(
                    step.allow_empty || !step.node_order.is_empty(),
                    "empty_host_selection"
                );
            }
        }
        let executors = |step: &Step| -> Vec<&Node> {
            if step.operation.location() == converyn_plugin_api::Location::Controller {
                controller_node.iter().collect()
            } else {
                let mut nodes: Vec<_> = nodes.iter().collect();
                if !step.node_order.is_empty() {
                    nodes.sort_by_key(|n| {
                        step.node_order
                            .iter()
                            .position(|id| id == &n.uid)
                            .unwrap_or(usize::MAX)
                    });
                }
                nodes
            }
        };

        crate::secrets::validate(&self.parameters)?;
        for values in self.workflow_values.values() {
            crate::secrets::validate(values)?;
        }
        ensure!(
            self.project.as_ref().is_none_or(|p| valid_id(p)),
            "invalid project"
        );
        ensure!(valid_id(&self.owner_uid), "invalid_owner_uid");
        ensure!(
            (1..=MAX_NODES).contains(&self.batch_size)
                && (1..=MAX_NODES).contains(&self.download_parallel),
            "invalid batch size"
        );
        ensure!(
            self.playbook.api_version == "converyn/v1" && valid_id(&self.playbook.name),
            "invalid playbook version or name"
        );
        ensure!(
            (1..=512).contains(&self.playbook.steps.len()),
            "playbook must contain 1..=64 steps"
        );
        let mut step_ids = BTreeSet::new();
        let mut phase = Phase::Prepare;
        for step in &self.playbook.steps {
            ensure!(
                valid_id(&step.id) && step_ids.insert(&step.id),
                "invalid or duplicate step ID"
            );
            ensure!(
                step.phase >= phase,
                "phases must be prepare, rollout, verify in order"
            );
            phase = step.phase;
            step.operation.validate()?;
            ensure!(
                step.max_parallel
                    .is_none_or(|n| (1..=MAX_NODES).contains(&n)),
                "invalid_parallel_limit"
            );
            match (step.phase, step.scope) {
                (Phase::Rollout, Scope::OncePerRun) => {
                    bail!("once_per_run belongs in prepare or verify")
                }
                (Phase::Prepare | Phase::Verify, Scope::OncePerBatch) => {
                    bail!("once_per_batch belongs in rollout")
                }
                _ => {}
            }
            if has_node_steps
                && step.operation.location() == converyn_plugin_api::Location::Controller
            {
                ensure!(
                    step.scope != Scope::PerNode,
                    "mixed controller steps use once_per_run or once_per_batch"
                );
            }
            let selected = executors(step)
                .into_iter()
                .filter(|n| step.selects(n))
                .count();
            ensure!(
                selected > 0 || (self.inventory.is_some() && step.allow_empty),
                "step {} matches no nodes",
                step.id
            );
            if selected == 0 {
                continue;
            }
            if step.scope != Scope::PerNode {
                ensure!(
                    selected == 1,
                    "step {} must select exactly one executor",
                    step.id
                );
            }
        }
        let resolve = |step: &Step, node: &Node| -> Operation {
            let mut operation = step.operation.clone();
            if let Operation::Exec {
                environment,
                fixed_environment,
                ..
            } = &mut operation
            {
                let mut values = self.parameters.clone();
                values.extend(environment.clone());
                // Explicit workflow globals apply to all scripts on this node.
                if let Some(globals) = self.workflow_values.get(&node.uid) {
                    values.extend(globals.clone());
                }
                values.extend(std::mem::take(fixed_environment));
                *environment = values;
            }
            operation
        };
        let mut expansion = Vec::new();
        for step in &self.playbook.steps {
            for node in executors(step) {
                if step.selects(node) {
                    let operation = resolve(step, node);
                    operation.validate()?;
                    if operation != step.operation {
                        expansion.push((&step.id, &node.uid, operation));
                    }
                }
            }
        }
        let sha256 = if expansion.is_empty() {
            digest(&self)
        } else {
            digest(&(&self, &expansion))
        };
        let mut work = Vec::new();
        let mut wave = 0;
        let rollout_nodes: Vec<Node> = if has_node_steps {
            self.rollout_nodes()
        } else {
            controller_node.iter().cloned().collect()
        }
        .iter()
        .filter(|node| {
            self.playbook.steps.iter().any(|s| {
                s.phase == Phase::Rollout
                    && s.scope == Scope::PerNode
                    && (!has_node_steps
                        || s.operation.location() == converyn_plugin_api::Location::Node)
                    && s.selects(node)
            })
        })
        .cloned()
        .collect();
        for step in self.playbook.steps.iter().filter(|s| {
            s.phase == Phase::Rollout
                && s.scope == Scope::PerNode
                && s.operation.location() == converyn_plugin_api::Location::Node
        }) {
            let batches: Vec<_> = step
                .node_order
                .iter()
                .filter_map(|uid| {
                    rollout_nodes
                        .iter()
                        .position(|n| &n.uid == uid)
                        .map(|i| i / self.batch_size)
                })
                .collect();
            ensure!(
                batches.windows(2).all(|pair| pair[0] <= pair[1]),
                "host_group_order_conflicts_with_rollout_batches"
            );
        }
        ensure!(
            !rollout_nodes.is_empty()
                || !self
                    .playbook
                    .steps
                    .iter()
                    .any(|s| s.scope == Scope::OncePerBatch),
            "batch hooks require per-node rollout targets"
        );
        // Member order is explicit input. Never derive ordinal selections from hash-map order.
        for phase in [Phase::Prepare, Phase::Rollout, Phase::Verify] {
            let batches: Vec<Option<(usize, &[Node])>> = if phase == Phase::Rollout {
                rollout_nodes
                    .chunks(self.batch_size)
                    .enumerate()
                    .map(Some)
                    .collect()
            } else {
                vec![None]
            };
            for batch in batches {
                for step in self.playbook.steps.iter().filter(|s| s.phase == phase) {
                    for node in executors(step) {
                        if !step.selects(node) {
                            continue;
                        }
                        if let Some((_, members)) = batch
                            && step.scope == Scope::PerNode
                            && step.operation.location() == converyn_plugin_api::Location::Node
                            && !members.iter().any(|n| n.uid == node.uid)
                        {
                            continue;
                        }
                        let batch_id = batch.map(|(i, _)| i as u32);
                        work.push(Work {
                            outputs: step.outputs.clone(),
                            inputs: step.inputs.clone(),
                            input_sha256: None,
                            id: digest(&(&sha256, &step.id, &node.uid, batch_id)),
                            step_id: step.id.clone(),
                            node_uid: node.uid.clone(),
                            phase,
                            batch: batch_id,
                            wave,
                            operation: resolve(step, node),
                        });
                    }
                    wave += 1;
                }
            }
        }
        ensure!(!work.is_empty(), "no_work_selected");
        super::outputs::validate_graph(&work, &self.playbook.steps)?;
        let plan = Plan {
            format: 4,
            input: self,
            sha256,
            work,
        };
        ensure!(
            serde_json::to_vec(&plan)?.len() <= MAX_PLAN_BYTES,
            "plan exceeds 1 MiB control-message limit"
        );
        Ok(plan)
    }
}

impl Plan {
    pub fn has_node_work(&self) -> bool {
        self.work
            .iter()
            .any(|w| w.operation.location() == converyn_plugin_api::Location::Node)
    }
    /// Actual Agents and external systems have separate lock namespaces; groups never lock.
    pub fn lock_keys(&self) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        for work in &self.work {
            if work.operation.location() == converyn_plugin_api::Location::Node {
                keys.insert(format!("node/{}", work.node_uid));
            }
        }
        if let Some(target) = &self.input.target {
            keys.insert(format!("external/{}", target.uid));
        }
        keys
    }

    pub fn verify_with(&self, contracts: &dyn converyn_plugin_api::Contracts) -> Result<()> {
        self.verify()?;
        for work in &self.work {
            if let Operation::Exec {
                origin: Some(origin),
                script_sha256,
                resources,
                ..
            } = &work.operation
            {
                contracts.validate_script(origin, script_sha256, resources)?;
            }
            if let Operation::Plugin { call } = &work.operation {
                contracts.validate_call(call)?;
                if call.location == converyn_plugin_api::Location::Controller
                    && let Some(target) = &self.input.target
                {
                    ensure!(
                        contracts.bind_target(call, &target.spec)? == **call,
                        "plugin target differs from frozen plan"
                    );
                }
            }
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<()> {
        ensure!(
            self.format == 4 && self.input.clone().compile()? == *self,
            "plan was modified after compilation"
        );
        Ok(())
    }
}
