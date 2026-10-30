//! Converyn 运行时：基础设施与插件装配的宿主；纯领域规则由 core 承担。
pub mod artifacts;
pub mod controller;
pub mod credentials;
pub mod executors;
pub mod interface;
pub mod log_store;
pub mod nodes;
pub mod orchestration;
pub mod storage;

use converyn_core::orchestration::machine;
pub mod plugins;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = machine::Command,
        R = machine::Reply,
        SnapshotData = storage::snapshot_file::SnapshotFile,
);

/// 所有应用写入先固定插件契约结果；共识重放不调用当前插件注册表。
#[derive(Clone)]
pub struct Raft {
    inner: openraft::Raft<TypeConfig>,
    store: storage::store::Store,
    admission: controller::admission::Admission,
    diagnostics: std::sync::Arc<controller::admission::Diagnostics>,
}

type WriteReply = openraft::raft::ClientWriteResponse<TypeConfig>;
type RaftWriteError =
    openraft::error::RaftError<u64, openraft::error::ClientWriteError<u64, openraft::BasicNode>>;

#[derive(Debug, thiserror::Error)]
pub enum SubmissionError {
    #[error("write_queue_full")]
    Full,
    #[error("submission_outcome_unknown; query the original intent before retrying")]
    Pending,
    #[error("consensus write failed: {0}")]
    Raft(#[from] Box<RaftWriteError>),
    #[error("write task interrupted: {0}")]
    Interrupted(#[from] tokio::task::JoinError),
}

fn recovery_write(command: &machine::Command) -> bool {
    use machine::Command;
    match command {
        Command::Authorized { command, .. } | Command::Prepared { command, .. } => {
            recovery_write(command)
        }
        Command::Identity { .. }
        | Command::Report { .. }
        | Command::ObserveNode { .. }
        | Command::Preparation { .. }
        | Command::Controlplane { .. }
        | Command::SetCertificateBinding { .. }
        | Command::Cancel { .. }
        | Command::PauseRun { .. }
        | Command::Resolve { .. }
        | Command::MarkUncertain { .. }
        | Command::ReconcileDrift { .. }
        | Command::ReconcileDeployment { .. }
        | Command::History { .. } => true,
        _ => false,
    }
}
impl std::ops::Deref for Raft {
    type Target = openraft::Raft<TypeConfig>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl Raft {
    pub fn with_store(inner: openraft::Raft<TypeConfig>, store: storage::store::Store) -> Self {
        Self {
            inner,
            store,
            admission: Default::default(),
            diagnostics: Default::default(),
        }
    }
    pub fn write_diagnostics(&self) -> serde_json::Value {
        use std::sync::atomic::Ordering::Relaxed;
        let d = &self.diagnostics;
        let (total, ordinary) = self.admission.in_flight();
        serde_json::json!({"limit":32,"ordinary_limit":24,"in_flight":total,"ordinary_in_flight":ordinary,"submitted":d.submitted.load(Relaxed),"committed":d.committed.load(Relaxed),"conflicts":d.conflicts.load(Relaxed),"queue_full":d.full.load(Relaxed),"outcome_pending":d.pending.load(Relaxed),"failures":d.failures.load(Relaxed),"last_success_ms":d.last_success_ms.load(Relaxed),"last_failure_ms":d.last_failure_ms.load(Relaxed)})
    }
    pub async fn client_write(
        &self,
        command: machine::Command,
    ) -> Result<WriteReply, SubmissionError> {
        let recovery = recovery_write(&command);
        let store = self.store.clone();
        let raft = self.inner.clone();
        let diagnostics = self.diagnostics.clone();
        use std::sync::atomic::Ordering::Relaxed;
        let write = self
            .admission
            .start(recovery, async move {
                diagnostics.submitted.fetch_add(1, Relaxed);
                let result: Result<WriteReply, Box<RaftWriteError>> = async {
                    let command = store.prepare_command(command).await.map_err(|e| {
                        Box::new(openraft::error::RaftError::Fatal(
                            openraft::error::Fatal::StorageError(e),
                        ))
                    })?;
                    raft.client_write(command).await.map_err(Box::new)
                }
                .await;
                match &result {
                    Ok(reply) if reply.data.ok => {
                        diagnostics.committed.fetch_add(1, Relaxed);
                        diagnostics
                            .last_success_ms
                            .store(crate::executors::host::now_ms(), Relaxed);
                    }
                    Ok(reply) => {
                        diagnostics.conflicts.fetch_add(1, Relaxed);
                        tracing::debug!(code=%reply.data.code, "state machine rejected submission");
                    }
                    Err(error) => diagnostics.failure(error),
                }
                result
            })
            .ok_or_else(|| {
                self.diagnostics.full.fetch_add(1, Relaxed);
                SubmissionError::Full
            })?;
        // Timeout detaches the operation. Both permits remain owned by the task
        // until preparation and the actual consensus submission have completed.
        match tokio::time::timeout(std::time::Duration::from_secs(3), write).await {
            Ok(Ok(result)) => result.map_err(SubmissionError::Raft),
            Ok(Err(error)) => {
                self.diagnostics.failure(&error);
                Err(SubmissionError::Interrupted(error))
            }
            Err(_) => {
                self.diagnostics.pending.fetch_add(1, Relaxed);
                Err(SubmissionError::Pending)
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
