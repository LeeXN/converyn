//! 仅根据已提交的工作项状态归纳 Run 状态。
use super::*;
use std::sync::Arc;

impl Run {
    /// Parallel work may finish out of order, but workflow-local groups dispatch
    /// in their frozen order even when Agents poll concurrently.
    pub fn ordered_predecessor_pending(&self, work: &crate::domain::Work) -> bool {
        self.plan.input.inventory.is_some()
            && self
                .plan
                .work
                .iter()
                .take_while(|w| w.id != work.id)
                .any(|w| {
                    w.wave == work.wave
                        && w.step_id == work.step_id
                        && self.attempts[&w.id].status == Status::Pending
                })
    }
}

pub(super) fn refresh(run: &mut Run) {
    let failed = run.attempts.values().any(|a| a.status == Status::Failed);
    run.cancel_requested |= run.attempts.values().any(|a| a.status == Status::Canceled);
    if run.cancel_requested || failed {
        for a in run.attempts.values_mut() {
            let a = Arc::make_mut(a);
            match a.status {
                Status::Pending => a.status = Status::Canceled,
                Status::Authorized | Status::Running => a.status = Status::CancelRequested,
                _ => {}
            }
        }
    }
    run.status = if run.attempts.values().all(|a| a.status == Status::Succeeded) {
        Status::Succeeded
    } else if run
        .attempts
        .values()
        .any(|a| a.status == Status::NeedsAttention)
    {
        Status::NeedsAttention
    } else if run.attempts.values().all(|a| a.status.terminal()) {
        if failed {
            Status::Failed
        } else {
            Status::Canceled
        }
    } else if run.cancel_requested || failed {
        Status::CancelRequested
    } else if run.pause_requested
        && !run.attempts.values().any(|a| {
            matches!(
                a.status,
                Status::Authorized | Status::Running | Status::CancelRequested
            )
        })
    {
        Status::Paused
    } else if run.preparing {
        Status::Preparing
    } else {
        Status::Running
    };
}

/// Every terminal transition, including failed input materialization, releases its original reservations.
pub(super) fn refresh_unlock(
    run: &mut Run,
    locks: &mut std::collections::BTreeMap<String, String>,
) {
    refresh(run);
    if run.status.terminal() {
        for key in run.plan.lock_keys() {
            if locks.get(&key) == Some(&run.id) {
                locks.remove(&key);
            }
        }
    }
}
