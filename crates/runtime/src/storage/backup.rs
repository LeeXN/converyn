//! SQLite disaster-recovery carriers exclude local credentials and Raft votes.
//! Recovery pauses work until the original controllers are fenced and evidence reviewed.
use crate::storage::store::Store;
use anyhow::{Result, ensure};
use converyn_core::orchestration::machine::{Command, State, Status};
use std::path::Path;
pub fn quarantine(original: &State) -> State {
    let mut state = original.clone();
    if let Some(uid) = state.controlplane.active.take()
        && let Some(operation) = state.controlplane.operations.get_mut(&uid)
    {
        operation.condition = "disaster_restore_paused".into();
        operation.diagnostic = Some("review replacement membership after recovery".into());
        operation.revision += 1;
    }
    state.administration.maintenance.extend(
        state
            .registry
            .nodes
            .values()
            .filter(|n| !n.retired)
            .map(|n| n.uid.clone()),
    );
    let ids: Vec<_> = state
        .runs
        .values()
        .filter(|r| !r.status.terminal())
        .map(|r| r.id.clone())
        .collect();
    for id in ids {
        let work: Vec<_> = state.runs[&id]
            .attempts
            .iter()
            .filter(|(_, a)| {
                matches!(
                    a.status,
                    Status::Authorized | Status::Running | Status::CancelRequested
                )
            })
            .map(|(id, _)| id.clone())
            .collect();
        for work_id in work {
            state.apply(Command::MarkUncertain {
                run_id: id.clone(),
                work_id,
                expected_revision: state.runs[&id].revision,
                reason: Some("disaster_restore_requires_execution_evidence".into()),
            });
        }
        let run = std::sync::Arc::make_mut(state.runs.get_mut(&id).unwrap());
        run.pause_requested = true;
        run.revision += 1;
        if matches!(
            run.status,
            Status::Pending | Status::Preparing | Status::Running
        ) {
            run.status = Status::Paused;
        }
    }
    for policy in state.drift.values_mut() {
        policy.interval_seconds = None;
        policy.condition = "disaster_restore_paused".into();
    }
    state
}
pub fn inspect(path: &Path) -> Result<(String, String)> {
    ensure!(
        crate::storage::carrier::sqlite(path)?,
        "backup must use a SQLite carrier"
    );
    let sha = crate::storage::carrier::verify_footer(path)?;
    let (_, cluster, kind) = crate::storage::carrier::validate(path)?;
    ensure!(kind == "backup", "not a logical backup");
    Ok((sha, cluster))
}
pub async fn restore(
    path: &Path,
    directory: &Path,
    node_id: u64,
    old_control_plane_fenced: bool,
) -> Result<()> {
    restore_with_keyring(path, directory, node_id, old_control_plane_fenced, None).await
}
pub async fn restore_with_keyring(
    path: &Path,
    directory: &Path,
    node_id: u64,
    old_control_plane_fenced: bool,
    keyring: Option<&Path>,
) -> Result<()> {
    ensure!(
        old_control_plane_fenced,
        "restore requires fencing the old control plane to prevent two controllers issuing work"
    );
    ensure!(
        node_id > 0 && !directory.exists(),
        "restore requires a positive member ID and a new data directory"
    );
    let (sha, cluster) = inspect(path)?;
    let (machine, _, _) = crate::storage::carrier::validate(path)?;
    if !machine.state.administration.secrets.is_empty() {
        let keys = crate::credentials::outputs::Keyring::load(keyring.ok_or_else(|| {
            anyhow::anyhow!("encrypted Secrets require --keyring from the original installation")
        })?)?;
        for (name, versions) in &machine.state.administration.secrets {
            for (version, sealed) in versions {
                crate::credentials::secrets::open(&keys, name, version, sealed)?;
            }
        }
    }
    let store = Store::open(directory, node_id)?;
    store.bind_cluster(&cluster).await?;
    store
        .restore_carrier_state(quarantine(&machine.state), &sha, path)
        .await?;
    Ok(())
}
