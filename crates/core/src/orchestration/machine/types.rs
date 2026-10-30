//! 复制协议与状态数据，不持有运行时资源。
use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::Plan;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Preparing,
    Paused,
    Authorized,
    Running,
    CancelRequested,
    NeedsAttention,
    Succeeded,
    Failed,
    Canceled,
}

impl Status {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attempt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialized: Option<crate::model::outputs::Materialized>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorized_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub receipt: Option<crate::runner::Receipt>,
    pub status: Status,
    pub grant: Option<String>,
    pub boot_id: Option<String>,
    pub session_id: Option<String>,
    pub last_sequence: u64,
    pub reports: BTreeMap<u64, String>,
}

impl Default for Attempt {
    fn default() -> Self {
        Self {
            completed_ms: None,
            materialized: None,
            authorized_ms: None,
            reason: None,
            receipt: None,
            status: Status::Pending,
            grant: None,
            boot_id: None,
            session_id: None,
            last_sequence: 0,
            reports: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Run {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_key: Option<crate::model::outputs::PublicKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_nodes: Option<std::collections::BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preparing: bool,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub prepared: std::collections::BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub preparation_errors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "zero")]
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pause_requested: bool,
    pub id: String,
    pub plan: Arc<Plan>,
    pub status: Status,
    pub cancel_requested: bool,
    pub revision: u64,
    pub attempts: BTreeMap<String, Arc<Attempt>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Access {
        change: crate::identities::access::Change,
    },
    RetryInputs {
        run_id: String,
        expected_revision: u64,
    },
    Materialize {
        run_id: String,
        work_id: String,
        expected_revision: u64,
        now_ms: u64,
    },
    Workflow {
        change: crate::workflow::Change,
    },
    Personnel {
        change: crate::identities::personnel::Change,
    },
    Prepared {
        command: Box<Command>,
        contracts: crate::model::contracts::Journal,
    },
    History {
        change: crate::history::Change,
    },
    Controlplane {
        change: crate::controlplane::Change,
    },
    Preparation {
        change: crate::preparation::Change,
    },

    PauseRun {
        run_id: String,
        paused: bool,
        expected_revision: u64,
    },
    Authorized {
        authorization: crate::registration::Authorization,
        command: Box<Command>,
    },
    Identity {
        change: crate::registration::Change,
    },
    ConfigureDrift {
        uid: String,
        interval_seconds: Option<u64>,
    },
    ReconcileDrift {
        uid: String,
        now_ms: u64,
        expected_generation: u64,
    },
    Administer {
        change: crate::administration::Change,
    },
    SetCertificateBinding {
        change: crate::identity::Change,
    },
    ApplyDeployment {
        uid: String,
        plan: Box<crate::domain::Plan>,
    },
    ReconcileDeployment {
        uid: String,
    },
    ObserveNode {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diagnostic: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inventory: Option<crate::model::telemetry::Snapshot>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<crate::model::telemetry::Snapshot>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inventory_checked_ms: Option<u64>,
        #[serde(default)]
        capabilities: Vec<converyn_plugin_api::Identity>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proof: Option<crate::registration::Proof>,
        #[serde(default)]
        hardware_sha256: Option<String>,
        node_uid: String,
        boot_id: String,
        session_id: String,
        observed_ms: u64,
    },
    PutScript {
        sha256: String,
        content: String,
    },
    CreateRun {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_key: Option<crate::model::outputs::PublicKey>,
        run_id: String,
        idempotency_key: String,
        plan: Box<Plan>,
    },
    Claim {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_sha256: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proof: Option<crate::registration::Proof>,
        #[serde(default)]
        authorized_ms: Option<u64>,
        run_id: String,
        work_id: String,
        node_uid: String,
        grant: String,
        boot_id: String,
        session_id: String,
    },
    Report {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proof: Option<crate::registration::Proof>,
        run_id: String,
        work_id: String,
        node_uid: String,
        grant: String,
        sequence: u64,
        status: Status,
        #[serde(default)]
        receipt: Option<crate::runner::Receipt>,
    },
    Cancel {
        run_id: String,
    },
    MarkUncertain {
        #[serde(default)]
        reason: Option<String>,
        run_id: String,
        work_id: String,
        expected_revision: u64,
    },
    /// P0 has no automatic proof of an external side effect; resolution remains explicit and audited.
    Resolve {
        run_id: String,
        work_id: String,
        expected_revision: u64,
        status: Status,
        actor: String,
        evidence: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    pub code: String,
    pub run_id: Option<String>,
    pub revision: Option<u64>,
}

impl Reply {
    pub(super) fn error(code: &str) -> Self {
        Self {
            ok: false,
            code: code.into(),
            run_id: None,
            revision: None,
        }
    }
    pub(super) fn run(run: &Run) -> Self {
        Self {
            ok: true,
            code: "ok".into(),
            run_id: Some(run.id.clone()),
            revision: Some(run.revision),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    #[serde(default)]
    pub workflows: crate::workflow::Registry,
    #[serde(default)]
    pub personnel: crate::identities::personnel::Registry,
    #[serde(default)]
    pub log_pruned: BTreeMap<String, String>,
    #[serde(default)]
    pub log_store_id: Option<String>,
    #[serde(default)]
    pub log_manifests: BTreeMap<String, crate::model::remote_logs::Manifest>,
    #[serde(default)]
    pub cold_audit: BTreeMap<String, AuditHead>,
    #[serde(default)]
    pub retirement_proofs: BTreeMap<String, String>,
    #[serde(default)]
    pub targets: BTreeMap<String, crate::model::target::ExternalTarget>,
    #[serde(default, skip_serializing_if = "crate::history::History::empty")]
    pub history: crate::history::History,
    #[serde(default, skip_serializing_if = "crate::controlplane::Catalog::empty")]
    pub controlplane: crate::controlplane::Catalog,
    #[serde(default, skip_serializing_if = "zero")]
    pub run_sequence: u64,
    #[serde(default)]
    pub registry: crate::registration::Registry,
    #[serde(default)]
    pub drift: BTreeMap<String, crate::drift::Policy>,
    #[serde(default)]
    pub owner_projects: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub administration: crate::administration::Administration,
    #[serde(default)]
    pub identities: crate::identity::Identities,
    #[serde(default)]
    pub deployments: BTreeMap<String, Deployment>,
    #[serde(default)]
    pub observations: BTreeMap<String, NodeObservation>,
    #[serde(default)]
    pub scripts: BTreeMap<String, String>,
    pub node_groups: BTreeMap<String, crate::domain::NodeGroup>,
    pub runs: BTreeMap<String, Arc<Run>>,
    pub execution_locks: BTreeMap<String, String>,
    pub resource_owners: BTreeMap<String, String>,
    pub idempotency: BTreeMap<String, (String, String)>,
    pub audit: Vec<Resolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Deployment {
    pub uid: String,
    pub generation: u64,
    pub applied_generation: Option<u64>,
    pub last_started_generation: u64,
    pub plan: Arc<crate::domain::Plan>,
    pub active_run: Option<String>,
    pub last_run: Option<String>,
    pub condition: String,
}

impl Deployment {
    pub fn admission_pending(&self) -> bool {
        self.active_run.is_none()
            && self.last_started_generation < self.generation
            && matches!(self.condition.as_str(), "pending" | "node_busy")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<crate::model::telemetry::Snapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<crate::model::telemetry::Snapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory_checked_ms: Option<u64>,
    #[serde(default)]
    pub capabilities: Vec<converyn_plugin_api::Identity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_sha256: Option<String>,
    pub boot_id: String,
    pub session_id: String,
    pub observed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resolution {
    pub run_id: String,
    pub work_id: String,
    pub actor: String,
    pub evidence: String,
    pub status: Status,
}

fn zero(value: &u64) -> bool {
    *value == 0
}

/// Constant-size per-stream commitment to immutable audit rows held on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditHead {
    pub count: u64,
    pub sha256: String,
}

impl Command {
    pub fn unwrapped(&self) -> &Self {
        match self {
            Self::Authorized { command, .. } | Self::Prepared { command, .. } => {
                command.unwrapped()
            }
            _ => self,
        }
    }
}

impl Run {
    /// Only a pure pre-execution failure can requeue its never-started suffix.
    /// Completed work, generated secrets and all existing receipts stay frozen.
    pub fn can_retry_inputs(&self) -> bool {
        self.status == Status::Failed
            && !self.cancel_requested
            && self.preparation_errors.is_empty()
            && self.attempts.values().any(|a| {
                a.status == Status::Failed
                    && a.reason
                        .as_deref()
                        .is_some_and(|s| s.starts_with("input_materialization:"))
            })
            && self.attempts.values().all(|a| {
                a.status == Status::Succeeded
                    || (matches!(
                        a.status,
                        Status::Failed | Status::Canceled | Status::Pending
                    ) && a.grant.is_none()
                        && a.receipt.is_none()
                        && a.last_sequence == 0)
            })
    }
}
