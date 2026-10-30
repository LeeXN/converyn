use converyn_core::orchestration::machine::{Command, State};
use converyn_runtime::credentials::security::Auth;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
fn auth() -> Auth {
    Auth {
        admin: "a".repeat(48),
        certificates: BTreeMap::new(),
        peer: "b".repeat(48),
        agents: BTreeMap::from([
            ("node-a".into(), "d".repeat(48)),
            ("node-b".into(), "e".repeat(48)),
        ]),
    }
}
#[test]
fn static_transport_identities_use_separate_namespaces() {
    let a = auth();
    a.validate().unwrap();
    assert_eq!(a.principal(&a.admin).as_deref(), Some("admin"));
    assert_eq!(a.principal(&a.peer).as_deref(), Some("peer"));
    assert_eq!(
        a.principal(&a.agents["node-a"]).as_deref(),
        Some("node:node-a")
    );
    assert_eq!(a.principal("unknown"), None);
}
#[test]
fn credentials_cannot_alias_roles() {
    let mut a = auth();
    a.peer = a.admin.clone();
    assert!(a.validate().is_err());
}
#[test]
fn script_definitions_verified_bounded_and_idempotent() {
    let mut s = State::default();
    let content = "echo hello\n".to_owned();
    let digest = hex::encode(Sha256::digest(content.as_bytes()));
    for _ in 0..2 {
        assert!(
            s.apply(Command::PutScript {
                sha256: digest.clone(),
                content: content.clone()
            })
            .ok
        );
    }
    assert_eq!(s.scripts.len(), 1);
    assert!(
        !s.apply(Command::PutScript {
            sha256: digest,
            content: "echo changed\n".into()
        })
        .ok
    );
    let content = "x".repeat(65537);
    let sha256 = hex::encode(Sha256::digest(content.as_bytes()));
    assert!(!s.apply(Command::PutScript { sha256, content }).ok);
}
#[test]
fn old_state_decodes_without_new_maps() {
    let mut v = serde_json::to_value(State::default()).unwrap();
    v.as_object_mut().unwrap().remove("scripts");
    v.as_object_mut().unwrap().remove("deployments");
    v.as_object_mut().unwrap().remove("observations");
    let s: State = serde_json::from_value(v).unwrap();
    assert!(s.scripts.is_empty() && s.observations.is_empty());
}

#[test]
fn certificate_and_credential_must_name_the_same_principal() {
    let mut a = auth();
    assert!(a.validate_bindings().is_err());
    for (index, name) in ["admin", "peer", "node:node-a", "node:node-b"]
        .iter()
        .enumerate()
    {
        a.certificates.insert(
            name.to_string(),
            [format!("{index:064x}")].into_iter().collect(),
        );
    }
    a.validate_bindings().unwrap();
    assert!(a.bound(&a.agents["node-a"], &format!("{:064x}", 2)));
    assert!(!a.bound(&a.agents["node-a"], &format!("{:064x}", 3)));
    assert!(!a.bound(&a.admin, &format!("{:064x}", 2)));
    assert!(!a.bound("invalid", &format!("{:064x}", 2)));
}

#[test]
fn replicated_certificate_rotation_revokes_old_leaf_and_survives_restore() {
    use converyn_core::identities::identity::{Change, Identities};
    let old = "a".repeat(64);
    let new = "b".repeat(64);
    let mut identities = Identities::default();
    let overlap = Change {
        principal: "node:node-a".into(),
        expected_revision: 0,
        fingerprints: [old, new.clone()].into_iter().collect(),
        evidence: "staged certificate replacement".into(),
    };
    assert_eq!(identities.change(overlap.clone()), Ok(1));
    assert_eq!(identities.change(overlap), Ok(1));
    let cutover = Change {
        principal: "node:node-a".into(),
        expected_revision: 1,
        fingerprints: [new.clone()].into_iter().collect(),
        evidence: "new leaf verified before old leaf revocation".into(),
    };
    assert_eq!(identities.change(cutover), Ok(2));
    let restored: Identities =
        serde_json::from_slice(&serde_json::to_vec(&identities).unwrap()).unwrap();
    assert_eq!(
        restored.bindings["node:node-a"].fingerprints,
        [new].into_iter().collect()
    );
    assert_eq!(restored.audit.len(), 2);
    assert_eq!(
        identities.change(Change {
            principal: "node:node-a".into(),
            expected_revision: 2,
            fingerprints: Default::default(),
            evidence: "node decommissioned".into()
        }),
        Ok(3)
    );
    assert!(identities.bindings["node:node-a"].fingerprints.is_empty());
}
