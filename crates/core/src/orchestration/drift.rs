//! Periodic reconciliation produces new immutable evidence, preserving rollout
//! batches while excluding every Shell and once-only hook from replay.
use crate::{
    domain::{Operation, Plan, Scope, digest},
    machine::{Command, State, Status},
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    #[serde(default)]
    pub configuration_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_evidence: Option<String>,
    pub interval_seconds: Option<u64>,
    pub last_started_ms: u64,
    pub sequence: u64,
    pub active_run: Option<String>,
    pub last_run: Option<String>,
    pub target_generation: u64,
    pub observed_generation: Option<u64>,
    pub condition: String,
}
pub fn plan(source: &Plan) -> Result<Plan, &'static str> {
    let mut input = source.input.clone();
    input.playbook.name = format!("drift-{}", &source.sha256[..16]);
    input.playbook.steps.retain(|s| {
        (s.scope == Scope::PerNode
            || s.operation.location() == converyn_plugin_api::Location::Controller)
            && (matches!(
                s.operation,
                Operation::File { .. }
                    | Operation::Binary { .. }
                    | Operation::Service { .. }
                    | Operation::HttpCheck { .. }
            ) || matches!(&s.operation,Operation::Plugin{call} if call.reconcilable))
    });
    input
        .compile()
        .map_err(|_| "no_reconcilable_native_resources")
}
impl State {
    pub fn configure_drift(
        &mut self,
        uid: &str,
        interval: Option<u64>,
    ) -> Result<(), &'static str> {
        let d = self.deployments.get(uid).ok_or("deployment_not_found")?;
        if let Some(seconds) = interval {
            if !(30..=86400).contains(&seconds) {
                return Err("invalid_drift_interval");
            }
            plan(&d.plan)?;
        }
        let policy = self.drift.entry(uid.into()).or_default();
        policy.interval_seconds = interval;
        if policy.active_run.is_none() {
            policy.condition = if interval.is_some() {
                "pending"
            } else {
                "disabled"
            }
            .into();
        }
        Ok(())
    }
    pub fn drift_due(&self, uid: &str, now: u64) -> bool {
        let Some(policy) = self.drift.get(uid) else {
            return false;
        };
        if let Some(id) = &policy.active_run {
            return self.runs.get(id).is_some_and(|r| {
                r.status.terminal()
                    || (r.status == Status::NeedsAttention && policy.condition != "needs_attention")
            });
        }
        if policy.condition == "scope_invalid" {
            return false;
        }
        let Some(interval) = policy.interval_seconds else {
            return false;
        };
        let Some(d) = self.deployments.get(uid) else {
            return false;
        };
        d.active_run.is_none()
            && d.applied_generation == Some(d.generation)
            && !d
                .plan
                .lock_keys()
                .iter()
                .any(|key| self.execution_locks.contains_key(key))
            && (policy.last_started_ms == 0
                || now.saturating_sub(policy.last_started_ms) >= interval * 1000)
    }
    pub fn reconcile_drift(
        &mut self,
        uid: &str,
        now: u64,
        generation: u64,
    ) -> Result<(), &'static str> {
        self.reconcile_drift_with(uid, now, generation, &converyn_plugin_api::NoPlugins)
    }
    pub fn reconcile_drift_with(
        &mut self,
        uid: &str,
        now: u64,
        generation: u64,
        contracts: &dyn converyn_plugin_api::Contracts,
    ) -> Result<(), &'static str> {
        let d = self
            .deployments
            .get(uid)
            .ok_or("deployment_not_found")?
            .clone();
        if d.generation != generation {
            return Err("stale_generation");
        }
        let mut policy = self.drift.get(uid).ok_or("drift_not_configured")?.clone();
        if let Some(id) = &policy.active_run {
            let run = self.runs.get(id).ok_or("drift_run_missing")?;
            if run.status == Status::NeedsAttention {
                policy.condition = "needs_attention".into();
            } else if run.status.terminal() {
                policy.condition = if run.status == Status::Succeeded {
                    "verified"
                } else {
                    "failed"
                }
                .into();
                if run.status == Status::Succeeded {
                    policy.observed_generation = Some(policy.target_generation);
                }
                policy.active_run = None;
            }
            self.drift.insert(uid.into(), policy);
            return Ok(());
        }
        if !self.drift_due(uid, now) {
            return Ok(());
        }
        let next = policy
            .sequence
            .checked_add(1)
            .ok_or("drift_sequence_exhausted")?;
        let id = format!("drift-{}-g{generation}-{next}", &digest(&uid)[..24]);
        let plan = plan(&d.plan)?;
        let reply = self.apply_with(
            Command::CreateRun {
                output_key: None,
                run_id: id.clone(),
                idempotency_key: id.clone(),
                plan: Box::new(plan),
            },
            contracts,
        );
        policy.last_started_ms = now.max(1);
        if reply.ok {
            if let Some(mut execution) = self
                .deployments
                .get(uid)
                .and_then(|d| d.last_run.as_ref())
                .and_then(|id| self.workflows.executions.get(id))
                .cloned()
            {
                execution.system = true;
                execution.submitted_ms = now;
                self.workflows.executions.insert(id.clone(), execution);
            }
            policy.sequence = next;
            policy.active_run = Some(id.clone());
            policy.last_run = Some(id);
            policy.target_generation = generation;
            policy.observed_generation = None;
            policy.condition = "checking".into();
        } else {
            policy.condition = reply.code;
        }
        self.drift.insert(uid.into(), policy);
        Ok(())
    }
}
