//! Replicated leaf-certificate rotation/revocation. Empty Node/publisher bindings
//! revoke access; overlapping fingerprints permit a staged certificate rotation.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Binding {
    pub revision: u64,
    pub fingerprints: BTreeSet<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Change {
    pub principal: String,
    pub expected_revision: u64,
    pub fingerprints: BTreeSet<String>,
    pub evidence: String,
}
impl Change {
    pub fn valid(&self) -> bool {
        (self.principal == "peer"
            || self.principal == "admin"
            || self
                .principal
                .strip_prefix("node:")
                .is_some_and(crate::domain::valid_id))
            && (!matches!(self.principal.as_str(), "admin" | "peer")
                || !self.fingerprints.is_empty())
            && self.fingerprints.len() <= 16
            && self.fingerprints.iter().all(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            && !self.evidence.trim().is_empty()
            && self.evidence.len() <= 4096
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identities {
    pub bindings: BTreeMap<String, Binding>,
    pub audit: Vec<Change>,
}
impl Identities {
    pub fn change(&mut self, change: Change) -> Result<u64, &'static str> {
        if !change.valid() {
            return Err("invalid_certificate_binding");
        }
        let revision = self
            .bindings
            .get(&change.principal)
            .map_or(0, |b| b.revision);
        if revision != change.expected_revision {
            if revision == change.expected_revision.saturating_add(1)
                && self
                    .bindings
                    .get(&change.principal)
                    .is_some_and(|b| b.fingerprints == change.fingerprints)
            {
                return Ok(revision);
            }
            return Err("certificate_binding_revision_conflict");
        }
        let next = revision
            .checked_add(1)
            .ok_or("certificate_binding_revision_exhausted")?;
        self.bindings.insert(
            change.principal.clone(),
            Binding {
                revision: next,
                fingerprints: change.fingerprints.clone(),
            },
        );
        self.audit.push(change);
        Ok(next)
    }
}
