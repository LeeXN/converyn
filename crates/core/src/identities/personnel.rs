//! Stable human identities and individually revocable certificate/token pairs.
//! Private keys and bearer values never enter replicated state.
use crate::{
    domain::{digest, valid_id},
    registration::Authorization,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_LIFETIME: u64 = 365 * 86400;
pub const MAX_LIFETIME: u64 = 3 * 365 * 86400;
pub const MAX_OVERLAP: u64 = 7 * 86400;

pub use super::access::Role;
use super::access::{Access, Action};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub uid: String,
    pub name: String,
    pub disabled: bool,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub uid: String,
    pub identity_uid: String,
    pub certificate_pem: String,
    pub fingerprint: String,
    pub token_sha256: String,
    pub not_before_ms: i64,
    pub expires_ms: u64,
    pub revoked_ms: Option<u64>,
    pub overlap_until_ms: Option<u64>,
}
impl Credential {
    pub fn active(&self, now: u64) -> bool {
        self.revoked_ms.is_none()
            && i128::from(now) >= i128::from(self.not_before_ms)
            && now < self.expires_ms
            && self.overlap_until_ms.is_none_or(|until| now < until)
    }
    pub fn validate(&self, now: u64) -> Result<()> {
        ensure!(
            self.certificate_pem.len() <= 16384 && !self.certificate_pem.contains("PRIVATE KEY"),
            "certificate_field_must_contain_only_public_certificates"
        );
        for item in rustls_pemfile::read_all(&mut self.certificate_pem.as_bytes()) {
            ensure!(
                matches!(item?, rustls_pemfile::Item::X509Certificate(_)),
                "certificate_field_must_contain_only_public_certificates"
            );
        }
        let cert = crate::certificate::info(&self.certificate_pem)?;
        ensure!(
            valid_id(&self.uid)
                && valid_id(&self.identity_uid)
                && crate::registration::valid_sha(&self.token_sha256)
                && cert.fingerprint == self.fingerprint
                && cert.expires_ms == self.expires_ms
                && cert.not_before_ms == self.not_before_ms
                && cert.client_auth
                && !cert.server_auth
                && !cert.is_ca
                && !cert.can_sign
                && self.active(now)
                && self.revoked_ms.is_none()
                && self.overlap_until_ms.is_none()
                && self.expires_ms.saturating_sub(now) <= MAX_LIFETIME * 1000,
            "invalid_personal_credential"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Registry {
    #[serde(default)]
    pub access: Access,
    pub identities: BTreeMap<String, Identity>,
    pub credentials: BTreeMap<String, Credential>,
    pub operations: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    AddIdentity {
        identity: Identity,
    },
    Create {
        #[serde(default)]
        super_admin: bool,
        identity: Identity,
        credential: Credential,
        request_id: String,
        now_ms: u64,
    },
    Update {
        uid: String,
        name: String,
        disabled: bool,
        expected_revision: u64,
    },
    Issue {
        credential: Credential,
        expected_revision: u64,
        rotate: bool,
        overlap_seconds: u64,
        request_id: String,
        now_ms: u64,
    },
    Revoke {
        uid: String,
        expected_revision: u64,
        now_ms: u64,
    },
}

impl Registry {
    pub fn has_active_admin(&self, now: u64) -> bool {
        self.identities.values().any(|i| {
            self.super_admin(&i.uid)
                && !i.disabled
                && self
                    .credentials
                    .values()
                    .any(|c| c.identity_uid == i.uid && c.active(now))
        })
    }
    pub fn authenticate(
        &self,
        fingerprint: &str,
        token_sha256: &str,
        now: u64,
    ) -> Option<(&Identity, &Credential)> {
        self.credentials.values().find_map(|c| {
            if c.fingerprint != fingerprint || c.token_sha256 != token_sha256 || !c.active(now) {
                return None;
            }
            self.identities
                .get(&c.identity_uid)
                .filter(|i| !i.disabled)
                .map(|i| (i, c))
        })
    }
    pub fn caller(&self, a: &Authorization, now: u64) -> Option<(&Identity, &Credential)> {
        self.authenticate(&a.fingerprint, a.token_sha256.as_deref()?, now)
            .filter(|(i, _)| i.uid == a.principal)
    }
    pub fn change(&mut self, change: Change) -> Result<()> {
        let operation = match &change {
            Change::Create { request_id, .. } | Change::Issue { request_id, .. } => {
                let mut value = serde_json::to_value(&change)?;
                value.as_object_mut().unwrap().remove("now_ms");
                Some((request_id.clone(), digest(&value)))
            }
            _ => None,
        };
        if let Some((id, sha)) = &operation {
            ensure!(valid_id(id), "invalid_request_id");
            if let Some(previous) = self.operations.get(id) {
                ensure!(previous == sha, "credential_idempotency_conflict");
                return Ok(());
            }
        }
        match change {
            Change::AddIdentity { identity } => {
                ensure!(
                    self.identities.len() < 10000
                        && valid_id(&identity.uid)
                        && identity.uid.starts_with("user-")
                        && !identity.name.trim().is_empty()
                        && identity.name.len() <= 256
                        && identity.revision == 1
                        && !identity.disabled
                        && !self.identities.contains_key(&identity.uid),
                    "invalid_identity"
                );
                self.identities.insert(identity.uid.clone(), identity);
            }
            Change::Create {
                identity,
                credential,
                super_admin,
                now_ms,
                ..
            } => {
                ensure!(
                    self.identities.len() < 10000 && self.credentials.len() < 50000,
                    "identity_limit"
                );
                ensure!(
                    valid_id(&identity.uid)
                        && identity.uid.starts_with("user-")
                        && !identity.name.trim().is_empty()
                        && identity.name.len() <= 256
                        && identity.revision == 1
                        && !identity.disabled
                        && credential.identity_uid == identity.uid
                        && !self.identities.contains_key(&identity.uid),
                    "invalid_identity"
                );
                self.validate_new(&credential, now_ms)?;
                let uid = identity.uid.clone();
                self.identities.insert(identity.uid.clone(), identity);
                if super_admin {
                    self.grant_initial(&uid, Role::SuperAdmin);
                }
                self.credentials.insert(credential.uid.clone(), credential);
            }
            Change::Update {
                uid,
                name,
                disabled,
                expected_revision,
            } => {
                ensure!(
                    !name.trim().is_empty() && name.len() <= 256,
                    "invalid_identity_name"
                );
                let identity = self
                    .identities
                    .get(&uid)
                    .ok_or_else(|| anyhow::anyhow!("identity_not_found"))?;
                if identity.name == name && identity.disabled == disabled {
                    return Ok(());
                }
                ensure!(
                    identity.revision == expected_revision,
                    "identity_revision_conflict"
                );
                ensure!(
                    !(disabled
                        && self.super_admin(&identity.uid)
                        && self
                            .identities
                            .values()
                            .filter(|i| self.super_admin(&i.uid) && !i.disabled)
                            .count()
                            <= 1),
                    "last_admin_cannot_be_disabled"
                );
                let identity = self.identities.get_mut(&uid).unwrap();
                identity.name = name;
                identity.disabled = disabled;
                identity.revision += 1;
            }
            Change::Issue {
                credential,
                expected_revision,
                rotate,
                overlap_seconds,
                now_ms,
                ..
            } => {
                ensure!(
                    self.credentials.len() < 50000
                        && overlap_seconds <= MAX_OVERLAP
                        && (rotate || overlap_seconds == 0),
                    "invalid_overlap"
                );
                self.validate_new(&credential, now_ms)?;
                let identity = self
                    .identities
                    .get(&credential.identity_uid)
                    .ok_or_else(|| anyhow::anyhow!("identity_not_found"))?;
                ensure!(
                    !identity.disabled && identity.revision == expected_revision,
                    "identity_revision_conflict"
                );
                ensure!(
                    rotate
                        || !self
                            .credentials
                            .values()
                            .any(|c| c.identity_uid == identity.uid && c.active(now_ms)),
                    "use_rotate_for_active_identity"
                );
                for old in self
                    .credentials
                    .values_mut()
                    .filter(|c| c.identity_uid == credential.identity_uid && c.revoked_ms.is_none())
                {
                    if overlap_seconds == 0 {
                        old.revoked_ms = Some(now_ms);
                    } else {
                        old.overlap_until_ms = Some(
                            old.overlap_until_ms
                                .unwrap_or(u64::MAX)
                                .min(old.expires_ms)
                                .min(now_ms.saturating_add(overlap_seconds * 1000)),
                        );
                    }
                }
                self.identities
                    .get_mut(&credential.identity_uid)
                    .unwrap()
                    .revision += 1;
                self.credentials.insert(credential.uid.clone(), credential);
            }
            Change::Revoke {
                uid,
                expected_revision,
                now_ms,
            } => {
                let c = self
                    .credentials
                    .get(&uid)
                    .ok_or_else(|| anyhow::anyhow!("credential_not_found"))?;
                if c.revoked_ms.is_some() {
                    return Ok(());
                }
                let i = self.identities.get(&c.identity_uid).unwrap();
                ensure!(
                    i.revision == expected_revision,
                    "identity_revision_conflict"
                );
                ensure!(
                    !(self.super_admin(&c.identity_uid)
                        && c.active(now_ms)
                        && !self.credentials.values().any(|other| other.uid != uid
                            && other.active(now_ms)
                            && self.super_admin(&other.identity_uid))),
                    "last_super_admin_credential_cannot_be_revoked"
                );
                self.identities.get_mut(&c.identity_uid).unwrap().revision += 1;
                self.credentials
                    .get_mut(&uid)
                    .unwrap()
                    .revoked_ms
                    .get_or_insert(now_ms);
            }
        }
        if let Some((id, sha)) = operation {
            self.operations.insert(id, sha);
        }
        Ok(())
    }
    fn validate_new(&self, c: &Credential, now: u64) -> Result<()> {
        c.validate(now)?;
        ensure!(
            !self.credentials.contains_key(&c.uid)
                && !self
                    .credentials
                    .values()
                    .any(|old| old.fingerprint == c.fingerprint
                        || old.token_sha256 == c.token_sha256),
            "credential_already_registered"
        );
        Ok(())
    }
}

impl crate::machine::State {
    /// Personal authorization is re-evaluated against committed role bindings.
    pub fn access_allows(&self, a: &Authorization, c: &crate::machine::Command) -> bool {
        use crate::machine::Command;
        if let Some((identity, credential)) = self.personnel.caller(a, a.observed_ms.unwrap_or(0)) {
            let uid = identity.uid.as_str();
            let allow =
                |action, workflow, preset| self.personnel.allows(uid, action, workflow, preset);
            return match c.unwrapped() {
                Command::Workflow {
                    change: crate::workflow::Change::Execute { actor, request, .. },
                } => {
                    actor.uid == identity.uid
                        && actor.name == identity.name
                        && actor.fingerprint == credential.fingerprint
                        && actor.credential_uid == credential.uid
                        && allow(
                            Action::Execute,
                            Some(&request.workflow_uid),
                            Some(&request.target),
                        )
                }
                Command::Workflow {
                    change: crate::workflow::Change::Register { workflow, .. },
                } => {
                    let old = self
                        .workflows
                        .workflows
                        .get(&workflow.uid)
                        .and_then(|w| self.workflows.revisions.get(&w.current));
                    // Only a new (target name, group UID) binding expands platform authority.
                    let changed = workflow.targets.iter().any(|(name, target)| {
                        target.node_group_uid.is_some()
                            && old
                                .and_then(|w| w.targets.get(name))
                                .is_none_or(|t| t.node_group_uid != target.node_group_uid)
                    });
                    allow(Action::Maintain, Some(&workflow.uid), None)
                        && (!changed || allow(Action::Platform, None, None))
                }
                Command::Workflow {
                    change: crate::workflow::Change::Enable { uid, .. },
                } => allow(Action::Maintain, Some(uid), None),
                Command::RetryInputs { run_id, .. }
                | Command::PauseRun { run_id, .. }
                | Command::Cancel { run_id } => self.can_control_run(uid, run_id),
                Command::Resolve { run_id, .. } | Command::MarkUncertain { run_id, .. } => {
                    self.workflows.executions.get(run_id).is_some_and(|e| {
                        allow(
                            Action::Control,
                            Some(&e.request.workflow_uid),
                            Some(&e.request.target),
                        )
                    })
                }
                Command::Personnel {
                    change:
                        Change::Create {
                            super_admin: true, ..
                        },
                } => self.personnel.super_admin(uid),
                Command::Personnel { change } => {
                    if let Change::Revoke { now_ms, .. } = change
                        && Some(*now_ms) != a.observed_ms
                    {
                        return false;
                    }
                    let target = match change {
                        Change::Update { uid, .. } => Some(uid.as_str()),
                        Change::Issue { credential, .. } => Some(credential.identity_uid.as_str()),
                        Change::Revoke { uid, .. } => self
                            .personnel
                            .credentials
                            .get(uid)
                            .map(|c| c.identity_uid.as_str()),
                        _ => None,
                    };
                    allow(Action::Access, None, None)
                        && (!target.is_some_and(|p| self.personnel.assigned_super_admin(p))
                            || self.personnel.super_admin(uid))
                }
                Command::Access { change } => {
                    // Only super administrators can change super-administrator membership.
                    let touches_super = match change {
                        super::access::Change::PutBinding { binding, .. } => {
                            binding.role == Role::SuperAdmin
                                || self
                                    .personnel
                                    .access
                                    .bindings
                                    .get(&binding.uid)
                                    .is_some_and(|b| b.role == Role::SuperAdmin)
                        }
                        super::access::Change::DeleteBinding { uid, .. } => self
                            .personnel
                            .access
                            .bindings
                            .get(uid)
                            .is_some_and(|b| b.role == Role::SuperAdmin),
                        super::access::Change::PutGroup { group, .. } => {
                            self.personnel.access.bindings.values().any(|b| {
                                b.role == Role::SuperAdmin
                                    && b.subject == super::access::Subject::Group(group.uid.clone())
                            })
                        }
                        _ => false,
                    };
                    allow(Action::Access, None, None)
                        && (!touches_super || self.personnel.super_admin(uid))
                }
                Command::CreateRun { .. }
                | Command::ApplyDeployment { .. }
                | Command::PutScript { .. } => false,
                Command::SetCertificateBinding { .. } => self.personnel.super_admin(uid),
                _ => allow(Action::Platform, None, None),
            };
        }
        if self.personnel.identities.contains_key(&a.principal) {
            return false;
        }
        if a.principal == "peer" {
            return true;
        }
        let Some(now) = a.observed_ms else {
            return false;
        };
        let command = c.unwrapped();
        if a.principal == "admin" {
            return match command {
                Command::Personnel {
                    change:
                        Change::Create {
                            super_admin: true, ..
                        },
                } => self.personnel.identities.is_empty(),
                Command::Personnel {
                    change: Change::Issue { credential, .. },
                } => {
                    !self.personnel.has_active_admin(now)
                        && self
                            .personnel
                            .identities
                            .get(&credential.identity_uid)
                            .is_some_and(|i| self.personnel.super_admin(&i.uid) && !i.disabled)
                }
                _ => false,
            };
        }
        let Some(uid) = a.principal.strip_prefix("node:") else {
            return false;
        };
        if self
            .registry
            .nodes
            .get(uid)
            .is_some_and(|n| n.retired || a.token_sha256.as_ref() != Some(&n.token_sha256))
        {
            return false;
        }
        let proof_allowed = |proof: &Option<crate::registration::Proof>, heartbeat| {
            proof.as_ref().is_some_and(|p| {
                p.principal == a.principal
                    && p.fingerprint == a.fingerprint
                    && a.token_sha256.as_ref() == Some(&p.token_sha256)
                    && self.identity_current(uid, p, heartbeat)
            })
        };
        match command {
            Command::Claim {
                node_uid, proof, ..
            }
            | Command::Report {
                node_uid, proof, ..
            } => node_uid == uid && proof_allowed(proof, false),
            Command::ObserveNode {
                node_uid, proof, ..
            } => node_uid == uid && proof_allowed(proof, true),
            Command::Preparation {
                change:
                    crate::preparation::Change::Prepared {
                        node_uid, proof, ..
                    }
                    | crate::preparation::Change::Failed {
                        node_uid, proof, ..
                    },
            } => node_uid == uid && proof_allowed(proof, false),
            Command::Identity {
                change:
                    crate::registration::Change::Renew { node_uid, .. }
                    | crate::registration::Change::Activate { node_uid, .. },
            } => node_uid == uid,
            Command::Administer {
                change: crate::administration::Change::LogFinal { manifest },
            } => manifest.node_uid == uid,
            _ => false,
        }
    }
}

impl crate::machine::State {
    pub fn can_view_run(&self, uid: &str, run: &str) -> bool {
        self.personnel.super_admin(uid)
            || self.workflows.executions.get(run).is_some_and(|e| {
                self.personnel.allows(
                    uid,
                    Action::View,
                    Some(&e.request.workflow_uid),
                    Some(&e.request.target),
                )
            })
    }
    pub fn can_control_run(&self, uid: &str, run: &str) -> bool {
        self.personnel.super_admin(uid)
            || self.workflows.executions.get(run).is_some_and(|e| {
                self.personnel.allows(
                    uid,
                    Action::Control,
                    Some(&e.request.workflow_uid),
                    Some(&e.request.target),
                ) || (e.actor.uid == uid
                    && self.personnel.allows(
                        uid,
                        Action::Execute,
                        Some(&e.request.workflow_uid),
                        Some(&e.request.target),
                    ))
            })
    }
}
