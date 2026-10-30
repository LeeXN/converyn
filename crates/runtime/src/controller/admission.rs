//! Capacity belongs to the durable operation, not to the HTTP waiter. Recovery
//! traffic keeps eight slots even when ordinary submissions fill their budget.
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{sync::Semaphore, task::JoinHandle};
use tracing::Instrument;

#[derive(Clone)]
pub(crate) struct Admission {
    total: Arc<Semaphore>,
    ordinary: Arc<Semaphore>,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            total: Arc::new(Semaphore::new(32)),
            ordinary: Arc::new(Semaphore::new(24)),
        }
    }
}
impl Admission {
    pub(crate) fn in_flight(&self) -> (usize, usize) {
        (
            32 - self.total.available_permits(),
            24 - self.ordinary.available_permits(),
        )
    }
    pub(crate) fn start<T: Send + 'static>(
        &self,
        recovery: bool,
        operation: impl Future<Output = T> + Send + 'static,
    ) -> Option<JoinHandle<T>> {
        // Acquire ordinary capacity first so a rejected ordinary request cannot
        // momentarily consume a slot reserved for a receipt or identity renewal.
        let ordinary = if recovery {
            None
        } else {
            Some(self.ordinary.clone().try_acquire_owned().ok()?)
        };
        let total = self.total.clone().try_acquire_owned().ok()?;
        Some(tokio::spawn(
            async move {
                let _ordinary = ordinary;
                let _total = total;
                operation.await
            }
            .instrument(tracing::Span::current()),
        ))
    }
}

#[derive(Default)]
pub(crate) struct Diagnostics {
    pub(crate) submitted: AtomicU64,
    pub(crate) committed: AtomicU64,
    pub(crate) conflicts: AtomicU64,
    pub(crate) full: AtomicU64,
    pub(crate) pending: AtomicU64,
    pub(crate) failures: AtomicU64,
    pub(crate) last_success_ms: AtomicU64,
    pub(crate) last_failure_ms: AtomicU64,
    last_warning_ms: AtomicU64,
}
impl Diagnostics {
    pub(crate) fn failure(&self, error: &dyn std::fmt::Display) {
        self.failures.fetch_add(1, Ordering::Relaxed);
        let now = crate::executors::host::now_ms();
        self.last_failure_ms.store(now, Ordering::Relaxed);
        let last = self.last_warning_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= 10_000
            && self
                .last_warning_ms
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            tracing::warn!(%error, failures=self.failures.load(Ordering::Relaxed), "consensus submission failed; repeated warnings limited to once per ten seconds");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn ordinary_saturation_preserves_recovery_slots_and_total_bound() {
        let admission = Admission::default();
        let mut release = Vec::new();
        let mut jobs = Vec::new();
        for index in 0..32 {
            let (tx, rx) = oneshot::channel();
            jobs.push(admission.start(index >= 24, rx).unwrap());
            release.push(tx);
            if index == 23 {
                assert!(admission.start(false, async {}).is_none());
            }
        }
        assert!(admission.start(true, async {}).is_none());
        assert!(admission.start(false, async {}).is_none());
        for tx in release {
            tx.send(()).unwrap();
        }
        for job in jobs {
            job.await.unwrap().unwrap();
        }
        assert!(admission.start(false, async {}).unwrap().await.is_ok());
    }

    #[tokio::test]
    async fn disconnect_and_timeout_do_not_release_running_write_capacity() {
        let admission = Admission::default();
        let (tx, rx) = oneshot::channel();
        let mut job = admission.start(false, rx).unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), &mut job)
                .await
                .is_err()
        );
        drop(job); // Dropping an HTTP waiter detaches, rather than aborts, its write.
        assert_eq!(admission.total.available_permits(), 31);
        assert_eq!(admission.ordinary.available_permits(), 23);
        tx.send(()).unwrap();
        for _ in 0..10 {
            tokio::task::yield_now().await;
            if admission.total.available_permits() == 32 {
                break;
            }
        }
        assert_eq!(admission.total.available_permits(), 32);
        assert_eq!(admission.ordinary.available_permits(), 24);
    }
}
