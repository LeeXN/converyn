//! One-use bootstrap grants and public identity records are replicated; private
//! keys and bearer values are generated on the operator/Node and never enter Raft.
use crate::{
    certificate::Signed,
    domain::{digest, valid_id},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bootstrap {
    pub id: String,
    pub node_uid: String,
    pub token_sha256: String,
    pub csr_sha256: String,
    pub certificate: Signed,
    pub expires_ms: u64,
    pub recover: bool,
    pub consumed: Option<String>,
    #[serde(default)]
    pub enrolled_certificate: Option<Signed>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingRenewal {
    pub id: String,
    pub csr_sha256: String,
    pub certificate: Signed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Node {
    pub uid: String,
    pub hardware_sha256: String,
    pub token_sha256: String,
    pub certificate: Signed,
    pub enrollment_id: String,
    pub enrollment_digest: String,
    pub pending: Option<PendingRenewal>,
    pub retired: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Registry {
    pub bootstraps: BTreeMap<String, Arc<Bootstrap>>,
    pub nodes: BTreeMap<String, Arc<Node>>,
    pub token_index: BTreeMap<String, String>,
    pub audit: Vec<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Bootstrap {
        grant: Box<Bootstrap>,
        now_ms: u64,
    },
    Enroll {
        grant_id: String,
        bootstrap_sha256: String,
        bootstrap_fingerprint: String,
        node: Box<Node>,
        now_ms: u64,
    },
    Renew {
        node_uid: String,
        pending: Box<PendingRenewal>,
        current_fingerprint: String,
        now_ms: u64,
    },
    Activate {
        node_uid: String,
        request_id: String,
        fingerprint: String,
        now_ms: u64,
    },
    Retire {
        node_uid: String,
        expected_fingerprint: String,
        evidence: String,
    },
    ExpireBootstrap {
        now_ms: u64,
    },
}
pub fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_certificate(c: &Signed, now: u64) -> bool {
    c.certificate_pem.len() <= 16384
        && c.expires_ms > now
        && crate::certificate::info(&c.certificate_pem).is_ok_and(|info| {
            info.fingerprint == c.fingerprint
                && info.expires_ms == c.expires_ms
                && !info.is_ca
                && info.client_auth
                && !info.server_auth
        })
}
impl crate::machine::State {
    fn bind_registered_certificate(
        &mut self,
        node: &str,
        fingerprints: std::collections::BTreeSet<String>,
        evidence: &str,
    ) -> Result<(), &'static str> {
        let principal = format!("node:{node}");
        let revision = self
            .identities
            .bindings
            .get(&principal)
            .map_or(0, |b| b.revision);
        self.identities.change(crate::identity::Change {
            principal,
            expected_revision: revision,
            fingerprints,
            evidence: evidence.into(),
        })?;
        Ok(())
    }
    pub fn register(&mut self, change: Change) -> Result<(), &'static str> {
        match change {
            Change::Bootstrap { grant, now_ms } => {
                if !valid_id(&grant.id)
                    || !valid_id(&grant.node_uid)
                    || !valid_sha(&grant.token_sha256)
                    || !valid_sha(&grant.csr_sha256)
                    || !valid_certificate(&grant.certificate, now_ms)
                    || grant.expires_ms <= now_ms
                    || grant.expires_ms > now_ms.saturating_add(3600000)
                    || grant.expires_ms > grant.certificate.expires_ms
                    || grant.consumed.is_some()
                    || grant.enrolled_certificate.is_some()
                {
                    return Err("invalid_bootstrap_grant");
                }
                if let Some(old) = self.registry.bootstraps.get(&grant.id) {
                    return if old.node_uid == grant.node_uid
                        && old.token_sha256 == grant.token_sha256
                        && old.csr_sha256 == grant.csr_sha256
                        && old.recover == grant.recover
                    {
                        Ok(())
                    } else {
                        Err("bootstrap_idempotency_conflict")
                    };
                }
                if self.registry.nodes.contains_key(&grant.node_uid) && !grant.recover {
                    return Err("node_already_registered");
                }
                if self.registry.bootstraps.len() >= 3000 {
                    return Err("bootstrap_quota");
                }
                self.registry.audit.push(serde_json::json!({"action":"bootstrap","id":grant.id,"node_uid":grant.node_uid,"expires_ms":grant.expires_ms,"recover":grant.recover}));
                self.registry
                    .bootstraps
                    .insert(grant.id.clone(), Arc::from(grant));
            }
            Change::Enroll {
                grant_id,
                bootstrap_sha256,
                bootstrap_fingerprint,
                node,
                now_ms,
            } => {
                if self
                    .personnel
                    .credentials
                    .values()
                    .any(|p| p.token_sha256 == node.token_sha256)
                {
                    return Err("credential_role_alias");
                }
                let grant = self
                    .registry
                    .bootstraps
                    .get(&grant_id)
                    .ok_or("bootstrap_not_found")?;
                if grant.token_sha256 != bootstrap_sha256
                    || grant.certificate.fingerprint != bootstrap_fingerprint
                    || grant.node_uid != node.uid
                    || grant.expires_ms <= now_ms
                {
                    return Err("bootstrap_rejected");
                }
                if let Some(id) = &grant.consumed {
                    return if id == &node.enrollment_id
                        && self
                            .registry
                            .nodes
                            .get(&node.uid)
                            .is_some_and(|old| old.enrollment_digest == node.enrollment_digest)
                    {
                        Ok(())
                    } else {
                        Err("bootstrap_already_consumed")
                    };
                }
                if !valid_id(&node.enrollment_id)
                    || !valid_id(&node.uid)
                    || !valid_sha(&node.hardware_sha256)
                    || !valid_sha(&node.token_sha256)
                    || !valid_sha(&node.enrollment_digest)
                    || !valid_certificate(&node.certificate, now_ms)
                    || node.pending.is_some()
                    || node.retired
                {
                    return Err("invalid_node_identity");
                }
                if let Some(old) = self.registry.nodes.get(&node.uid)
                    && (!grant.recover || old.hardware_sha256 != node.hardware_sha256)
                {
                    return Err("node_identity_conflict");
                }
                if self
                    .observations
                    .get(&node.uid)
                    .and_then(|o| o.hardware_sha256.as_ref())
                    .is_some_and(|hardware| hardware != &node.hardware_sha256)
                {
                    return Err("node_hardware_mismatch");
                }
                if self.observations.iter().any(|(uid, o)| {
                    let retired = self.registry.nodes.get(uid).is_some_and(|n| n.retired);
                    let revoked = self
                        .identities
                        .bindings
                        .get(&format!("node:{uid}"))
                        .is_some_and(|b| b.fingerprints.is_empty());
                    uid != &node.uid
                        && o.hardware_sha256.as_ref() == Some(&node.hardware_sha256)
                        && !retired
                        && !revoked
                }) {
                    return Err("hardware_already_registered");
                }
                if !self.registry.nodes.contains_key(&node.uid)
                    && self
                        .observations
                        .get(&node.uid)
                        .is_some_and(|o| o.hardware_sha256.is_none())
                    && self.runs.values().any(|r| {
                        !r.status.terminal() && r.plan.work.iter().any(|w| w.node_uid == node.uid)
                    })
                {
                    return Err("unregistered_identity_in_use");
                }
                if self.registry.nodes.values().any(|old| {
                    !old.retired
                        && old.uid != node.uid
                        && old.hardware_sha256 == node.hardware_sha256
                }) {
                    return Err("hardware_already_registered");
                }
                if self
                    .registry
                    .token_index
                    .get(&node.token_sha256)
                    .is_some_and(|uid| uid != &node.uid)
                {
                    return Err("credential_reuse");
                }
                if self.registry.nodes.len() >= 3000 && !self.registry.nodes.contains_key(&node.uid)
                {
                    return Err("node_identity_quota");
                }
                self.bind_registered_certificate(
                    &node.uid,
                    [node.certificate.fingerprint.clone()].into_iter().collect(),
                    "Single-use enrollment certificate activated",
                )?;
                if let Some(old) = self.registry.nodes.get(&node.uid) {
                    self.registry.token_index.remove(&old.token_sha256);
                }
                self.registry
                    .token_index
                    .insert(node.token_sha256.clone(), node.uid.clone());
                let grant = Arc::make_mut(self.registry.bootstraps.get_mut(&grant_id).unwrap());
                grant.consumed = Some(node.enrollment_id.clone());
                grant.enrolled_certificate = Some(node.certificate.clone());
                self.registry.audit.push(serde_json::json!({"action":"enrolled","node_uid":node.uid,"request_id":node.enrollment_id,"certificate":node.certificate.fingerprint,"hardware":node.hardware_sha256}));
                if let Some(observation) = self.observations.get_mut(&node.uid) {
                    observation.observed_ms = 0;
                }
                self.registry
                    .nodes
                    .insert(node.uid.clone(), Arc::from(node));
            }
            Change::Renew {
                node_uid,
                pending,
                current_fingerprint,
                now_ms,
            } => {
                let node = self
                    .registry
                    .nodes
                    .get(&node_uid)
                    .ok_or("node_not_registered")?;
                if let Some(old) = &node.pending {
                    return if old.id == pending.id && old.csr_sha256 == pending.csr_sha256 {
                        Ok(())
                    } else {
                        Err("renewal_already_pending")
                    };
                }
                if node.retired
                    || node.certificate.fingerprint != current_fingerprint
                    || !self
                        .identities
                        .bindings
                        .get(&format!("node:{node_uid}"))
                        .is_some_and(|b| b.fingerprints.contains(&current_fingerprint))
                {
                    return Err("node_credential_revoked");
                }
                if !valid_id(&pending.id)
                    || !valid_sha(&pending.csr_sha256)
                    || !valid_certificate(&pending.certificate, now_ms)
                {
                    return Err("invalid_renewal");
                }
                self.bind_registered_certificate(
                    &node_uid,
                    [current_fingerprint, pending.certificate.fingerprint.clone()]
                        .into_iter()
                        .collect(),
                    "Stage certificate renewal with overlap",
                )?;
                Arc::make_mut(self.registry.nodes.get_mut(&node_uid).unwrap()).pending =
                    Some(*pending);
            }
            Change::Activate {
                node_uid,
                request_id,
                fingerprint,
                now_ms,
            } => {
                let node = self
                    .registry
                    .nodes
                    .get(&node_uid)
                    .ok_or("node_not_registered")?;
                let Some(pending) = node.pending.clone() else {
                    return if node.certificate.fingerprint == fingerprint {
                        Ok(())
                    } else {
                        Err("renewal_not_pending")
                    };
                };
                if node.retired
                    || pending.id != request_id
                    || pending.certificate.fingerprint != fingerprint
                    || pending.certificate.expires_ms <= now_ms
                    || !self
                        .identities
                        .bindings
                        .get(&format!("node:{node_uid}"))
                        .is_some_and(|b| b.fingerprints.contains(&fingerprint))
                {
                    return Err("renewal_activation_rejected");
                }
                self.bind_registered_certificate(
                    &node_uid,
                    [fingerprint.clone()].into_iter().collect(),
                    "Renewed certificate proved possession; retire previous leaf",
                )?;
                let node = Arc::make_mut(self.registry.nodes.get_mut(&node_uid).unwrap());
                node.certificate = pending.certificate;
                node.pending = None;
                self.registry.audit.push(serde_json::json!({"action":"renewed","node_uid":node_uid,"fingerprint":fingerprint,"request_id":request_id}));
            }
            Change::Retire {
                node_uid,
                expected_fingerprint,
                evidence,
            } => {
                let node = self
                    .registry
                    .nodes
                    .get(&node_uid)
                    .ok_or("node_not_registered")?;
                if node.certificate.fingerprint != expected_fingerprint
                    || evidence.trim().is_empty()
                    || evidence.len() > 4096
                {
                    return Err("invalid_identity_retirement");
                }
                let retirement =
                    crate::domain::digest(&(&node_uid, &expected_fingerprint, &evidence));
                if node.retired {
                    return if self.retirement_proofs.get(&format!("node/{node_uid}"))
                        == Some(&retirement)
                    {
                        Ok(())
                    } else {
                        Err("identity_retirement_conflict")
                    };
                }
                if self
                    .node_groups
                    .values()
                    .any(|g| g.node_uids.contains(&node_uid))
                    || self.runs.values().any(|r| {
                        !r.status.terminal() && r.plan.work.iter().any(|w| w.node_uid == node_uid)
                    })
                {
                    return Err("node_still_in_use");
                }
                self.bind_registered_certificate(&node_uid, Default::default(), &evidence)?;
                let node = Arc::make_mut(self.registry.nodes.get_mut(&node_uid).unwrap());
                node.retired = true;
                self.registry.token_index.remove(&node.token_sha256);
                self.retirement_proofs
                    .insert(format!("node/{node_uid}"), retirement);
                self.registry.audit.push(
                    serde_json::json!({"action":"retired","node_uid":node_uid,"evidence":evidence}),
                );
            }
            Change::ExpireBootstrap { now_ms } => {
                self.registry
                    .bootstraps
                    .retain(|_, g| g.expires_ms > now_ms);
            }
        }
        Ok(())
    }
}
pub fn enrollment_digest(
    request_id: &str,
    node: &str,
    hardware: &str,
    token_sha: &str,
    csr_sha: &str,
) -> String {
    digest(&(request_id, node, hardware, token_sha, csr_sha))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Proof {
    pub principal: String,
    pub fingerprint: String,
    pub token_sha256: String,
    pub hardware_sha256: Option<String>,
    pub session: Option<String>,
}
impl crate::machine::State {
    pub fn identity_current(&self, node: &str, proof: &Proof, heartbeat: bool) -> bool {
        if proof.principal != format!("node:{node}") {
            return false;
        }
        let identity = self.registry.nodes.get(node);
        if identity.is_some_and(|n| n.retired || n.token_sha256 != proof.token_sha256) {
            return false;
        }
        if self
            .identities
            .bindings
            .get(&proof.principal)
            .is_some_and(|b| !b.fingerprints.contains(&proof.fingerprint))
        {
            return false;
        }
        let observation = self.observations.get(node);
        let hardware = identity
            .map(|n| n.hardware_sha256.as_str())
            .or_else(|| observation.and_then(|o| o.hardware_sha256.as_deref()));
        if hardware.is_some_and(|h| proof.hardware_sha256.as_deref() != Some(h)) {
            return false;
        }
        if hardware.is_some()
            && !heartbeat
            && observation.is_none_or(|o| proof.session.as_deref() != Some(o.session_id.as_str()))
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authorization {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub observed_ms: Option<u64>,
    #[serde(default)]
    pub token_sha256: Option<String>,
    pub principal: String,
    pub fingerprint: String,
}
