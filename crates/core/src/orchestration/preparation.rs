//! Artifact readiness is committed before acquiring a Cluster mutation lock.
//! Cached bytes are verified by the Agent; completion still requires execution
//! receipts. Current work and two successful artifact generations remain pinned.
use crate::{
    machine::{State, Status},
    registration::Proof,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Prepared {
        run_id: String,
        work_id: String,
        node_uid: String,
        sha256: String,
        size: u64,
        #[serde(default)]
        proof: Option<Proof>,
    },
    Failed {
        run_id: String,
        work_id: String,
        node_uid: String,
        reason: String,
        #[serde(default)]
        proof: Option<Proof>,
    },
    Admit {
        run_id: String,
        expected_revision: u64,
    },
}
impl State {
    pub(crate) fn workflow_nodes_busy(
        &self,
        plan: &crate::domain::Plan,
        except: Option<&str>,
    ) -> bool {
        let nodes: std::collections::BTreeSet<_> = plan
            .work
            .iter()
            .filter(|w| w.operation.location() == converyn_plugin_api::Location::Node)
            .map(|w| &w.node_uid)
            .collect();
        self.runs.values().any(|run| {
            except != Some(run.id.as_str())
                && !run.preparing
                && !run.status.terminal()
                && run.plan.work.iter().any(|w| {
                    w.operation.location() == converyn_plugin_api::Location::Node
                        && nodes.contains(&w.node_uid)
                })
        })
    }
    pub fn prepare(&mut self, change: Change) -> Result<(), &'static str> {
        match change {
            Change::Admit {
                run_id,
                expected_revision,
            } => {
                let run = self.runs.get(&run_id).ok_or("run_not_found")?;
                if !run.preparing
                    || run.status.terminal()
                    || run.pause_requested
                    || run.cancel_requested
                    || run.revision != expected_revision
                {
                    return Err("stale_preparation");
                }
                if run
                    .plan
                    .lock_keys()
                    .iter()
                    .any(|key| self.execution_locks.contains_key(key))
                {
                    return Err("node_busy");
                }
                if self.workflow_nodes_busy(&run.plan, Some(&run.id)) {
                    return Err("node_busy");
                }
                if let Err(code) = self.validate_host_scope(&run.plan) {
                    let run = std::sync::Arc::make_mut(self.runs.get_mut(&run_id).unwrap());
                    run.preparation_errors.insert(
                        "scope".into(),
                        "scope_invalid: refresh preview and submit a new Run".into(),
                    );
                    for attempt in run.attempts.values_mut() {
                        let attempt = std::sync::Arc::make_mut(attempt);
                        attempt.status = Status::Failed;
                        attempt.reason = Some(code.into());
                    }
                    run.status = Status::Failed;
                    run.revision += 1;
                    return Err(code);
                }
                if run
                    .plan
                    .work
                    .iter()
                    .any(|w| self.administration.maintenance.contains(&w.node_uid))
                {
                    return Err("node_in_maintenance");
                }
                if run
                    .plan
                    .work
                    .iter()
                    .any(|w| w.operation.artifact().is_some() && !run.prepared.contains(&w.id))
                {
                    return Err("artifacts_not_ready");
                }
                if run.plan.work.iter().any(|w| {
                    w.operation
                        .resource_keys(&w.node_uid)
                        .iter()
                        .any(|key| self.resource_owners.get(key) != Some(&run.plan.input.owner_uid))
                }) {
                    return Err("resource_owner_conflict");
                }
                let run = std::sync::Arc::make_mut(self.runs.get_mut(&run_id).unwrap());
                run.preparing = false;
                run.status = Status::Pending;
                run.revision += 1;
                for key in run.plan.lock_keys() {
                    self.execution_locks.insert(key, run_id.clone());
                }
            }
            Change::Prepared {
                run_id,
                work_id,
                node_uid,
                sha256,
                size,
                proof,
            } => {
                if proof
                    .as_ref()
                    .is_some_and(|p| !self.identity_current(&node_uid, p, false))
                {
                    return Err("node_identity_fenced");
                }
                let run =
                    std::sync::Arc::make_mut(self.runs.get_mut(&run_id).ok_or("run_not_found")?);
                let work = run
                    .plan
                    .work
                    .iter()
                    .find(|w| w.id == work_id && w.node_uid == node_uid)
                    .ok_or("wrong_node_or_work")?;
                let artifact = work
                    .operation
                    .artifact()
                    .ok_or("not_an_artifact_operation")?;
                if artifact.sha256 != sha256 || artifact.size != size {
                    return Err("artifact_evidence_mismatch");
                }
                if run.prepared.contains(&work_id) {
                    return Ok(());
                }
                if !run.preparing || run.status.terminal() {
                    return Err("preparation_not_active");
                }
                run.preparation_errors.remove(&work_id);
                run.prepared.insert(work_id);
                run.revision += 1;
            }
            Change::Failed {
                run_id,
                work_id,
                node_uid,
                reason,
                proof,
            } => {
                if proof
                    .as_ref()
                    .is_some_and(|p| !self.identity_current(&node_uid, p, false))
                {
                    return Err("node_identity_fenced");
                }
                let run =
                    std::sync::Arc::make_mut(self.runs.get_mut(&run_id).ok_or("run_not_found")?);
                if run.prepared.contains(&work_id) {
                    return Ok(());
                }
                if !run.preparing
                    || run.status.terminal()
                    || reason.is_empty()
                    || reason.len() > 2048
                {
                    return Err("invalid_preparation_failure");
                }
                if !run.plan.work.iter().any(|w| {
                    w.id == work_id && w.node_uid == node_uid && w.operation.artifact().is_some()
                }) {
                    return Err("wrong_node_or_work");
                }
                if run.preparation_errors.get(&work_id) != Some(&reason) {
                    run.preparation_errors.insert(work_id, reason);
                    run.revision += 1;
                }
            }
        }
        Ok(())
    }
    pub fn cache_pins(&self, node: &str) -> BTreeMap<String, u64> {
        let mut runs: Vec<_> = self
            .runs
            .values()
            .filter(|r| {
                r.artifact_nodes.as_ref().map_or_else(
                    || {
                        r.plan
                            .work
                            .iter()
                            .any(|w| w.node_uid == node && w.operation.artifact().is_some())
                    },
                    |nodes| nodes.contains(node),
                )
            })
            .collect();
        runs.sort_by_key(|r| std::cmp::Reverse(r.sequence));
        let mut kept: BTreeMap<&str, usize> = BTreeMap::new();
        let mut pins = BTreeMap::new();
        for run in runs {
            let retain = if !run.status.terminal() {
                true
            } else if run.status == Status::Succeeded {
                let count = kept.entry(&run.plan.input.owner_uid).or_default();
                *count += 1;
                *count <= 2 || run.sequence == 0
            } else {
                false
            };
            if retain {
                for work in run.plan.work.iter().filter(|w| w.node_uid == node) {
                    if let Some(a) = work.operation.artifact() {
                        pins.insert(a.sha256.clone(), a.size);
                    }
                }
            }
        }
        pins
    }
}
