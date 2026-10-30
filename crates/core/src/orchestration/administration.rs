//! Platform scope changes protect occupied Agents without owning workflow topology.
use crate::{
    domain::{NodeGroup, valid_id},
    machine::State,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Secret {
        name: String,
        version: String,
        sealed: crate::model::outputs::SealedValue,
    },
    Artifact {
        sha256: String,
        size: u64,
    },
    LogPruned {
        key: String,
        store_id: String,
        sha256: String,
    },
    LogStorage {
        store_id: String,
    },
    LogFinal {
        manifest: crate::model::remote_logs::Manifest,
    },
    FenceFloor {
        expected_sequence: u64,
        floor: u64,
        evidence: String,
    },
    Target {
        target: crate::model::target::ExternalTarget,
    },
    DeleteNodeGroup {
        uid: String,
        expected_revision: u64,
        reason: String,
    },
    MaintenanceWindow {
        window: MaintenanceWindow,
        expected_revision: u64,
        evidence: String,
    },
    CreateNodeGroup {
        group: NodeGroup,
        expected_revision: u64,
    },
    UpdateNodeGroup {
        group: NodeGroup,
        expected_revision: u64,
    },
    Maintenance {
        node_uid: String,
        enabled: bool,
    },
    Ownership {
        resource: String,
        expected_owner: String,
        expected_revision: u64,
        new_owner: Option<String>,
        evidence: String,
    },
}
/// A UTC blackout interval prevents new grants. Already authorized operations
/// keep their original grant and can report results throughout the window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceWindow {
    pub uid: String,
    pub revision: u64,
    pub node_uids: BTreeSet<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub enabled: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Administration {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<String, BTreeMap<String, crate::model::outputs::SealedValue>>,
    #[serde(default)]
    pub artifacts: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub retired_node_groups: BTreeMap<String, NodeGroup>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub windows: BTreeMap<String, MaintenanceWindow>,
    #[serde(default)]
    pub owner_revisions: BTreeMap<String, u64>,
    pub maintenance: BTreeSet<String>,
    pub audit: Vec<Change>,
}
impl Administration {
    pub fn window_until(&self, node: &str, now: Option<u64>) -> Option<u64> {
        self.windows
            .values()
            .filter(|w| {
                w.enabled
                    && w.node_uids.contains(node)
                    && now.is_none_or(|t| w.start_ms <= t && t < w.end_ms)
            })
            .map(|w| w.end_ms)
            .max()
    }
}
impl State {
    /// Preview and consensus admission use the same occupancy and ownership rules.
    pub fn execution_ready(&self, plan: &crate::domain::Plan) -> Result<(), &'static str> {
        self.validate_host_scope(plan)?;
        if plan.work.iter().any(|w| {
            w.operation.location() == converyn_plugin_api::Location::Node
                && self.administration.maintenance.contains(&w.node_uid)
        }) {
            return Err("node_in_maintenance");
        }
        if plan
            .lock_keys()
            .iter()
            .any(|key| self.execution_locks.contains_key(key))
            || self.workflow_nodes_busy(plan, None)
        {
            return Err("node_busy");
        }
        if plan.work.iter().any(|w| {
            w.operation.resource_keys(&w.node_uid).iter().any(|key| {
                self.resource_owners
                    .get(key)
                    .is_some_and(|owner| owner != &plan.input.owner_uid)
            })
        }) {
            return Err("resource_owner_conflict");
        }
        Ok(())
    }
    pub fn node_enrolled(&self, uid: &str) -> bool {
        self.registry.nodes.get(uid).is_some_and(|n| !n.retired)
    }
    pub fn node_occupied(&self, uid: &str) -> bool {
        self.runs.values().any(|r| {
            !r.preparing
                && !r.status.terminal()
                && r.plan.work.iter().any(|w| {
                    w.operation.location() == converyn_plugin_api::Location::Node
                        && w.node_uid == uid
                })
        })
    }
    /// Admission and recovery share this authority check. Unrelated revisions are harmless
    /// after submission; preview freshness is checked separately using the target digest.
    pub fn validate_host_scope(&self, plan: &crate::domain::Plan) -> Result<(), &'static str> {
        if let Some(hosts) = &plan.input.hosts {
            let group = self
                .node_groups
                .get(&hosts.node_group_uid)
                .ok_or("scope_invalid")?;
            if hosts
                .node_uids
                .iter()
                .any(|id| !group.node_uids.contains(id) || !self.node_enrolled(id))
            {
                return Err("scope_invalid");
            }
        }
        Ok(())
    }

    pub fn administer(&mut self, change: Change) -> Result<(), &'static str> {
        match &change {
            Change::Secret {
                name,
                version,
                sealed,
            } => {
                if !valid_id(name)
                    || !valid_id(version)
                    || sealed.plaintext_bytes > 65536
                    || sealed.ciphertext.len() > 131072
                    || sealed.ciphertext.is_empty()
                {
                    return Err("invalid_secret");
                }
                let versions = self.administration.secrets.entry(name.clone()).or_default();
                if let Some(old) = versions.get(version) {
                    return if old == sealed {
                        Ok(())
                    } else {
                        Err("secret_version_immutable")
                    };
                }
                versions.insert(version.clone(), sealed.clone());
            }
            Change::Artifact { sha256, size } => {
                if !crate::registration::valid_sha(sha256)
                    || *size == 0
                    || *size > crate::domain::MAX_ARTIFACT_SIZE
                {
                    return Err("invalid_managed_artifact");
                }
                if let Some(old) = self.administration.artifacts.get(sha256) {
                    return if old == size {
                        Ok(())
                    } else {
                        Err("artifact_size_immutable")
                    };
                }
                if self.administration.artifacts.len() >= 10000 {
                    return Err("managed_artifact_limit");
                }
                self.administration.artifacts.insert(sha256.clone(), *size);
            }
            Change::LogPruned {
                key,
                store_id,
                sha256,
            } => {
                let m = self.log_manifests.get(key).ok_or("log_manifest_missing")?;
                if m.store_id != *store_id || m.sha256 != *sha256 {
                    return Err("log_prune_manifest_mismatch");
                }
                if self.log_pruned.get(key) == Some(sha256) {
                    return Ok(());
                }
                self.log_pruned.insert(key.clone(), sha256.clone());
            }
            Change::LogStorage { store_id } => {
                if !crate::registration::valid_sha(store_id) {
                    return Err("invalid_log_store_id");
                }
                if let Some(old) = &self.log_store_id {
                    return if old == store_id {
                        Ok(())
                    } else {
                        Err("log_storage_identity_immutable")
                    };
                }
                self.log_store_id = Some(store_id.clone());
            }
            Change::LogFinal { manifest: m } => {
                if self.log_store_id.as_deref() != Some(m.store_id.as_str()) {
                    return Err("log_storage_binding_mismatch");
                }
                let run = self.runs.get(&m.run_id).ok_or("run_not_found")?;
                let work = run
                    .plan
                    .work
                    .iter()
                    .find(|w| w.id == m.work_id && w.node_uid == m.node_uid)
                    .ok_or("work_not_assigned")?;
                let _ = work;
                let a = run.attempts.get(&m.work_id).ok_or("attempt_not_found")?;
                let logs = a
                    .receipt
                    .as_ref()
                    .and_then(|r| r.logs.as_ref())
                    .ok_or("terminal_logs_required")?;
                let log = match m.stream.as_str() {
                    "stdout" => &logs.stdout,
                    "stderr" => &logs.stderr,
                    _ => return Err("invalid_log_stream"),
                };
                if !a.status.terminal() && a.status != crate::machine::Status::NeedsAttention {
                    return Err("terminal_receipt_required");
                }
                if a.grant.as_deref() != Some(&m.grant)
                    || m.bytes != log.stored_bytes
                    || m.sha256 != log.sha256
                    || m.truncated != log.truncated
                    || m.missing_ranges
                        .iter()
                        .any(|(a, b)| a >= b || *b > m.bytes || !a.is_multiple_of(65536))
                    || m.chunk_hashes
                        .iter()
                        .any(|s| !crate::registration::valid_sha(s))
                    || m.chunk_hashes.len() as u64
                        != m.missing_ranges
                            .first()
                            .map_or(m.chunks, |(start, _)| start / 65536)
                    || m.chunks != m.bytes.div_ceil(65536)
                    || !crate::registration::valid_sha(&m.store_id)
                {
                    return Err("log_manifest_receipt_mismatch");
                }
                let key = m.key();
                if let Some(old) = self.log_manifests.get(&key) {
                    return if old == m {
                        Ok(())
                    } else {
                        Err("immutable_log_manifest")
                    };
                }
                self.log_manifests.insert(key, m.clone());
            }
            Change::FenceFloor {
                expected_sequence,
                floor,
                evidence,
            } => {
                if *expected_sequence != self.run_sequence
                    || *floor <= self.run_sequence
                    || *floor == u64::MAX
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                {
                    return Err("invalid_fence_floor");
                }
                self.run_sequence = *floor;
            }

            Change::Target { target } => {
                target.validate().map_err(|_| "invalid_target")?;
                if let Some(old) = self.targets.get(&target.uid) {
                    if old != target {
                        return Err("target_identity_immutable");
                    } else {
                        return Ok(());
                    }
                }
                if self.targets.len() >= 10000 {
                    return Err("target_quota");
                }
                self.targets.insert(target.uid.clone(), target.clone());
            }

            Change::DeleteNodeGroup {
                uid,
                expected_revision,
                reason,
            } => {
                if self.administration.retired_node_groups.contains_key(uid) {
                    return if self.retirement_proofs.get(&format!("node-group/{uid}"))
                        == Some(&crate::domain::digest(&change))
                        || self.administration.audit.contains(&change)
                    {
                        Ok(())
                    } else {
                        Err("node_group_retired")
                    };
                }
                if reason.trim().is_empty() || reason.len() > 4096 {
                    return Err("invalid_node_group_retirement");
                }
                let group = self.node_groups.get(uid).ok_or("node_group_not_found")?;
                if group.revision != *expected_revision {
                    return Err("node_group_revision_conflict");
                }
                if self.workflows.workflows.values().any(|w| {
                    self.workflows.revisions.get(&w.current).is_some_and(|r| {
                        r.targets
                            .values()
                            .any(|t| t.node_group_uid.as_ref() == Some(uid))
                    })
                }) {
                    return Err("node_group_referenced_by_workflow");
                }
                if self.runs.values().any(|r| {
                    !r.preparing
                        && !r.status.terminal()
                        && r.plan
                            .input
                            .hosts
                            .as_ref()
                            .is_some_and(|h| &h.node_group_uid == uid)
                }) {
                    return Err("node_group_members_in_use");
                }
                self.administration
                    .retired_node_groups
                    .insert(uid.clone(), group.clone());
                self.retirement_proofs
                    .insert(format!("node-group/{uid}"), crate::domain::digest(&change));
                self.node_groups.remove(uid);
            }
            Change::MaintenanceWindow {
                window,
                expected_revision,
                evidence,
            } => {
                if !valid_id(&window.uid)
                    || window.node_uids.is_empty()
                    || window.node_uids.len() > 300
                    || window.start_ms >= window.end_ms
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                    || window.node_uids.iter().any(|n| !self.node_enrolled(n))
                {
                    return Err("invalid_maintenance_window");
                }
                if self.administration.windows.get(&window.uid) == Some(window) {
                    return Ok(());
                }
                if self
                    .administration
                    .windows
                    .get(&window.uid)
                    .map_or(0, |w| w.revision)
                    != *expected_revision
                    || expected_revision.checked_add(1) != Some(window.revision)
                {
                    return Err("maintenance_window_revision_conflict");
                }
                if self.administration.windows.len() >= 1000
                    && !self.administration.windows.contains_key(&window.uid)
                {
                    return Err("maintenance_window_quota");
                }
                self.administration
                    .windows
                    .insert(window.uid.clone(), window.clone());
            }
            Change::CreateNodeGroup {
                group,
                expected_revision,
            }
            | Change::UpdateNodeGroup {
                group,
                expected_revision,
            } => {
                let creating = matches!(&change, Change::CreateNodeGroup { .. });
                if creating && (*expected_revision != 0 || group.revision != 1) {
                    return Err("invalid_node_group_creation");
                }
                if !creating && *expected_revision == 0 {
                    return Err("node_group_not_found");
                }
                if self
                    .administration
                    .retired_node_groups
                    .contains_key(&group.uid)
                {
                    return Err("node_group_retired");
                }
                group.validate().map_err(|_| "invalid_node_group")?;
                if group.node_uids.iter().any(|id| !self.node_enrolled(id)) {
                    return Err("node_group_member_not_enrolled");
                }
                let old = self.node_groups.get(&group.uid);
                if old == Some(group) {
                    return Ok(());
                }
                if old.map_or(0, |g| g.revision) != *expected_revision
                    || expected_revision.checked_add(1) != Some(group.revision)
                {
                    return Err("node_group_revision_conflict");
                }
                if old.is_some_and(|g| {
                    g.node_uids.difference(&group.node_uids).any(|id| {
                        self.runs.values().any(|r| {
                            !r.preparing
                                && !r.status.terminal()
                                && r.plan.input.hosts.as_ref().is_some_and(|h| {
                                    h.node_group_uid == group.uid && h.node_uids.contains(id)
                                })
                        })
                    })
                }) {
                    return Err("node_group_members_in_use");
                }
                self.node_groups.insert(group.uid.clone(), group.clone());
            }
            Change::Maintenance { node_uid, enabled } => {
                if !self.node_enrolled(node_uid) {
                    return Err("node_not_registered");
                }
                if self.node_occupied(node_uid) {
                    return Err("node_busy");
                }
                if *enabled {
                    self.administration.maintenance.insert(node_uid.clone());
                } else {
                    self.administration.maintenance.remove(node_uid);
                }
            }
            Change::Ownership {
                resource,
                expected_owner,
                expected_revision,
                new_owner,
                evidence,
            } => {
                if !valid_id(expected_owner)
                    || new_owner.as_ref().is_some_and(|n| !valid_id(n))
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                {
                    return Err("invalid_ownership_change");
                }
                let node = resource.split('/').next().ok_or("invalid_resource")?;
                if !self.node_enrolled(node) {
                    return Err("node_not_registered");
                }
                if self.node_occupied(node) {
                    return Err("node_busy");
                }
                let current_revision = self
                    .administration
                    .owner_revisions
                    .get(resource)
                    .copied()
                    .unwrap_or(0);
                if current_revision != *expected_revision {
                    return Err("resource_revision_conflict");
                }
                let next_revision = current_revision
                    .checked_add(1)
                    .ok_or("resource_revision_exhausted")?;
                if self.resource_owners.get(resource) != Some(expected_owner) {
                    return Err("resource_owner_conflict");
                }
                self.administration
                    .owner_revisions
                    .insert(resource.clone(), next_revision);
                if let Some(owner) = new_owner {
                    self.resource_owners.insert(resource.clone(), owner.clone());
                } else {
                    self.resource_owners.remove(resource);
                }
            }
        }
        self.administration.audit.push(change);
        Ok(())
    }
}
