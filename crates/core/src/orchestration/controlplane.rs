//! Member replacement is a replicated, retryable intent. Catch-up precedes
//! voter promotion; completion is recorded only after a stable applied config.
use crate::machine::State;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Replace {
    pub uid: String,
    pub old_id: u64,
    pub new_id: u64,
    pub address: String,
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub expected_membership: String,
    pub voters: BTreeSet<u64>,
    pub evidence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Operation {
    pub intent: Replace,
    pub condition: String,
    pub diagnostic: Option<String>,
    pub revision: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Catalog {
    pub active: Option<String>,
    pub operations: BTreeMap<String, Operation>,
}
impl Catalog {
    pub fn empty(&self) -> bool {
        self.active.is_none() && self.operations.is_empty()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Request {
        intent: Replace,
        existing_fingerprints: BTreeSet<String>,
    },
    Observe {
        uid: String,
        expected_revision: u64,
        condition: String,
        diagnostic: Option<String>,
    },
}
fn fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl State {
    pub fn controlplane_change(&mut self, change: Change) -> Result<(), &'static str> {
        match change {
            Change::Request {
                intent,
                existing_fingerprints,
            } => {
                if let Some(old) = self.controlplane.operations.get(&intent.uid) {
                    return if old.intent == intent {
                        Ok(())
                    } else {
                        Err("member_intent_immutable")
                    };
                }
                if self.controlplane.active.is_some() {
                    return Err("member_operation_active");
                }
                if self.controlplane.operations.len() >= 1000 {
                    return Err("member_operation_quota");
                }
                if !crate::domain::valid_id(&intent.uid)
                    || intent.old_id == 0
                    || intent.new_id == 0
                    || intent.old_id == intent.new_id
                    || !matches!(intent.voters.len(), 3 | 5)
                    || !intent.voters.contains(&intent.old_id)
                    || intent.voters.contains(&intent.new_id)
                    || !fingerprint(&intent.old_fingerprint)
                    || !fingerprint(&intent.new_fingerprint)
                    || intent.old_fingerprint == intent.new_fingerprint
                    || !existing_fingerprints.contains(&intent.old_fingerprint)
                    || intent.evidence.trim().is_empty()
                    || intent.evidence.len() > 4096
                {
                    return Err("invalid_member_intent");
                }
                if self
                    .identities
                    .bindings
                    .get("peer")
                    .is_some_and(|b| b.fingerprints != existing_fingerprints)
                {
                    return Err("peer_trust_changed");
                }
                let mut accepted = existing_fingerprints;
                accepted.insert(intent.new_fingerprint.clone());
                let mut identities = self.identities.clone();
                let revision = identities.bindings.get("peer").map_or(0, |b| b.revision);
                identities.change(crate::identity::Change {
                    principal: "peer".into(),
                    expected_revision: revision,
                    fingerprints: accepted,
                    evidence: format!("Stage replacement {}: {}", intent.uid, intent.evidence),
                })?;
                self.identities = identities;
                self.controlplane.active = Some(intent.uid.clone());
                self.controlplane.operations.insert(
                    intent.uid.clone(),
                    Operation {
                        intent,
                        condition: "pending".into(),
                        diagnostic: None,
                        revision: 1,
                    },
                );
            }
            Change::Observe {
                uid,
                expected_revision,
                condition,
                diagnostic,
            } => {
                let operation = self
                    .controlplane
                    .operations
                    .get(&uid)
                    .ok_or("member_operation_missing")?;
                if self.controlplane.active.as_ref() != Some(&uid)
                    || operation.revision != expected_revision
                {
                    return Err("stale_member_observation");
                }
                if !matches!(
                    condition.as_str(),
                    "waiting_for_member" | "catching_up" | "changing_voters" | "succeeded"
                ) || diagnostic.as_ref().is_some_and(|v| v.len() > 2048)
                {
                    return Err("invalid_member_observation");
                }
                if condition == "succeeded" {
                    let mut identities = self.identities.clone();
                    let old = identities
                        .bindings
                        .get("peer")
                        .ok_or("peer_trust_missing")?;
                    let mut accepted = old.fingerprints.clone();
                    accepted.remove(&operation.intent.old_fingerprint);
                    identities.change(crate::identity::Change {
                        principal: "peer".into(),
                        expected_revision: old.revision,
                        fingerprints: accepted,
                        evidence: format!(
                            "Retire replaced controller {} after stable membership",
                            operation.intent.old_id
                        ),
                    })?;
                    self.identities = identities;
                    self.controlplane.active = None;
                }
                let operation = self.controlplane.operations.get_mut(&uid).unwrap();
                operation.condition = condition;
                operation.diagnostic = diagnostic;
                operation.revision += 1;
            }
        }
        Ok(())
    }
}
