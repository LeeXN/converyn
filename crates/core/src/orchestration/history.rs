//! Cold history remains in each replica's SQLite database. Only its small
//! index stays in the committed in-memory view; archiving never deletes proof.
use crate::{
    domain::digest,
    machine::{State, Status},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Run,
    Definition,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Definition => "definition",
        }
    }
}
pub fn key(kind: Kind, uid: &str) -> String {
    format!("{}/{}", kind.name(), uid)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<u64>,
    pub kind: Kind,
    pub uid: String,
    pub sha256: String,
    pub bytes: u64,
    pub revision: u64,
    pub owner: String,
    pub project: Option<String>,
    pub node_group_uid: Option<String>,
    pub target: String,
    pub sequence: u64,
    pub status: Option<Status>,
    pub plan_sha256: String,
    pub total: usize,
    pub completed: usize,
    pub archived_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct History {
    pub records: BTreeMap<String, std::sync::Arc<Record>>,
    pub keep_runs: usize,
    pub definition_hot_bytes: u64,
    pub configuration_revision: u64,
    pub audit: Vec<serde_json::Value>,
}
impl Default for History {
    fn default() -> Self {
        Self {
            records: BTreeMap::new(),
            keep_runs: 32,
            definition_hot_bytes: 2 * 1024 * 1024,
            configuration_revision: 0,
            audit: vec![],
        }
    }
}
impl History {
    pub fn empty(&self) -> bool {
        self == &Self::default()
    }
    pub fn get(&self, kind: Kind, uid: &str) -> Option<&Record> {
        self.records.get(&key(kind, uid)).map(AsRef::as_ref)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    /// Explicit operator cleanup, after exporting a verified recovery carrier.
    ClearRuns {
        expected_sequence: u64,
        backup_sha256: String,
        evidence: String,
    },
    Archive {
        kind: Kind,
        uid: String,
        expected_revision: u64,
        now_ms: u64,
    },
    Configure {
        expected_revision: u64,
        keep_runs: usize,
        definition_hot_bytes: u64,
        evidence: String,
    },
}
impl State {
    pub fn run_status(&self, uid: &str) -> Option<Status> {
        self.runs
            .get(uid)
            .map(|r| r.status)
            .or_else(|| self.history.get(Kind::Run, uid).and_then(|r| r.status))
    }
    pub fn has_definition(&self, sha: &str) -> bool {
        self.scripts.contains_key(sha) || self.history.get(Kind::Definition, sha).is_some()
    }
    pub fn run_protected(&self, uid: &str) -> bool {
        let Some(r) = self.runs.get(uid) else {
            return false;
        };
        if !r.status.terminal()
            || r.attempts.values().any(|a| {
                matches!(
                    a.status,
                    Status::Authorized
                        | Status::Running
                        | Status::CancelRequested
                        | Status::NeedsAttention
                )
            })
            || self
                .deployments
                .values()
                .any(|d| d.active_run.as_deref() == Some(uid) || d.last_run.as_deref() == Some(uid))
            || self
                .drift
                .values()
                .any(|d| d.active_run.as_deref() == Some(uid) || d.last_run.as_deref() == Some(uid))
        {
            return true;
        }
        if r.status == Status::Succeeded
            && r.plan.work.iter().any(|w| w.operation.artifact().is_some())
        {
            if r.sequence == 0 {
                return true;
            }
            let nodes = r
                .plan
                .work
                .iter()
                .filter(|w| w.operation.artifact().is_some())
                .map(|w| &w.node_uid)
                .collect::<std::collections::BTreeSet<_>>();
            for node in nodes {
                let newer = self
                    .runs
                    .values()
                    .filter(|other| {
                        other.status == Status::Succeeded
                            && other.plan.input.owner_uid == r.plan.input.owner_uid
                            && other.sequence > r.sequence
                            && other.artifact_nodes.as_ref().map_or_else(
                                || {
                                    other.plan.work.iter().any(|w| {
                                        &w.node_uid == node && w.operation.artifact().is_some()
                                    })
                                },
                                |nodes| nodes.contains(node),
                            )
                    })
                    .count();
                if newer < 2 {
                    return true;
                }
            }
        }
        false
    }
    pub fn history_change(&mut self, change: Change) -> Result<(), &'static str> {
        let fingerprint = crate::domain::digest(&change);
        match change {
            Change::ClearRuns {
                expected_sequence,
                backup_sha256,
                evidence,
            } => {
                let proof = format!("history-clear/{expected_sequence}");
                if self.retirement_proofs.get(&proof) == Some(&fingerprint) {
                    return Ok(());
                }
                if self.run_sequence != expected_sequence
                    || !crate::registration::valid_sha(&backup_sha256)
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                {
                    return Err("history_clear_precondition_failed");
                }
                if self.runs.values().any(|run| {
                    !run.status.terminal()
                        || run.attempts.values().any(|a| {
                            matches!(
                                a.status,
                                Status::Authorized
                                    | Status::Running
                                    | Status::CancelRequested
                                    | Status::NeedsAttention
                            )
                        })
                }) || !self.execution_locks.is_empty()
                    || self.deployments.values().any(|d| d.active_run.is_some())
                    || self.drift.values().any(|d| d.active_run.is_some())
                {
                    return Err("history_clear_requires_idle_execution");
                }
                let ids = self
                    .runs
                    .keys()
                    .cloned()
                    .chain(
                        self.history
                            .records
                            .values()
                            .filter(|r| r.kind == Kind::Run)
                            .map(|r| r.uid.clone()),
                    )
                    .chain(self.workflows.executions.keys().cloned())
                    .collect::<std::collections::BTreeSet<_>>();
                for id in &ids {
                    self.retirement_proofs
                        .insert(format!("run/{id}"), backup_sha256.clone());
                }
                // Keep sequence/fences, ownership and request deduplication.
                // Old request IDs remain retired even if reused on a new target.
                self.runs.clear();
                self.history.records.retain(|_, r| r.kind != Kind::Run);
                self.workflows.executions.clear();
                for d in self.deployments.values_mut() {
                    d.last_run = None;
                }
                for d in self.drift.values_mut() {
                    d.last_run = None;
                }
                self.retirement_proofs.insert(proof, fingerprint);
                self.history.audit.push(serde_json::json!({"action":"clear_runs","through_sequence":expected_sequence,"cleared_runs":ids.len(),"backup_sha256":backup_sha256,"evidence":evidence}));
            }
            Change::Configure {
                expected_revision,
                keep_runs,
                definition_hot_bytes,
                evidence,
            } => {
                if self.history.configuration_revision != expected_revision
                    || !(2..=1000).contains(&keep_runs)
                    || !(65536..=4 * 1024 * 1024).contains(&definition_hot_bytes)
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                {
                    return Err("invalid_history_configuration");
                }
                self.history.configuration_revision = self
                    .history
                    .configuration_revision
                    .checked_add(1)
                    .ok_or("history_revision_exhausted")?;
                self.history.keep_runs = keep_runs;
                self.history.definition_hot_bytes = definition_hot_bytes;
                self.history.audit.push(serde_json::json!({"revision":self.history.configuration_revision,"keep_runs":keep_runs,"definition_hot_bytes":definition_hot_bytes,"evidence":evidence}));
            }
            Change::Archive {
                kind,
                uid,
                expected_revision,
                now_ms,
            } => {
                let mut record = Record {
                    submitted_ms: None,
                    finished_ms: None,
                    kind,
                    uid: uid.clone(),
                    sha256: String::new(),
                    bytes: 0,
                    revision: expected_revision,
                    owner: String::new(),
                    project: None,
                    node_group_uid: None,
                    target: String::new(),
                    sequence: 0,
                    status: None,
                    plan_sha256: String::new(),
                    total: 0,
                    completed: 0,
                    archived_ms: now_ms,
                };
                match kind {
                    Kind::Run => {
                        let Some(r) = self.runs.get(&uid) else {
                            return if self.history.get(kind, &uid).is_some() {
                                Ok(())
                            } else {
                                Err("run_not_found")
                            };
                        };
                        if r.revision != expected_revision || self.run_protected(&uid) {
                            return Err("run_history_protected");
                        }
                        record.sha256 = digest(r);
                        record.bytes = serde_json::to_vec(r)
                            .map_err(|_| "history_encoding_failed")?
                            .len() as u64;
                        record.owner = r.plan.input.owner_uid.clone();
                        record.project = r.plan.input.project.clone();
                        record.node_group_uid = r
                            .plan
                            .input
                            .hosts
                            .as_ref()
                            .map(|h| h.node_group_uid.clone());
                        record.target = r
                            .plan
                            .input
                            .target
                            .as_ref()
                            .map_or(String::new(), |t| t.uid.clone());
                        record.submitted_ms = r.submitted_ms;
                        record.finished_ms = r.finished_ms;
                        record.sequence = r.sequence;
                        record.status = Some(r.status);
                        record.plan_sha256 = r.plan.sha256.clone();
                        record.total = r.attempts.len();
                        record.completed =
                            r.attempts.values().filter(|a| a.status.terminal()).count();
                    }

                    Kind::Definition => {
                        let Some(s) = self.scripts.get(&uid) else {
                            return if self.history.get(kind, &uid).is_some() {
                                Ok(())
                            } else {
                                Err("definition_not_found")
                            };
                        };
                        if expected_revision != 0 {
                            return Err("definition_revision_invalid");
                        }
                        record.sha256 = digest(s);
                        record.bytes = serde_json::to_vec(s)
                            .map_err(|_| "history_encoding_failed")?
                            .len() as u64;
                    }
                }
                if let Some(old) = self.history.get(kind, &uid)
                    && (old.sha256 != record.sha256 || old.revision != record.revision)
                {
                    return Err("immutable_history_conflict");
                }
                match kind {
                    Kind::Run => {
                        self.owner_projects
                            .entry(record.owner.clone())
                            .or_insert(record.project.clone());
                        self.runs.remove(&uid);
                    }

                    Kind::Definition => {
                        self.scripts.remove(&uid);
                    }
                }
                self.history
                    .records
                    .entry(key(kind, &uid))
                    .or_insert_with(|| std::sync::Arc::new(record));
            }
        }
        Ok(())
    }
}
