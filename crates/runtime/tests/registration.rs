use converyn_core::identities::registration::{Bootstrap, Change, Node, PendingRenewal, Proof};
use converyn_core::orchestration::machine::{Command, State};
use converyn_runtime::credentials::pki;
use sha2::{Digest, Sha256};
fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value))
}
fn grant(authority: &pki::Authority, id: &str, node: &str) -> Bootstrap {
    let (_, csr) = pki::request("bootstrap").unwrap();
    Bootstrap {
        id: id.into(),
        node_uid: node.into(),
        token_sha256: hash(id),
        csr_sha256: hash(&csr),
        certificate: authority.sign_client("bootstrap", &csr, 600).unwrap(),
        expires_ms: converyn_runtime::executors::host::now_ms() + 500000,
        recover: false,
        consumed: None,
        enrolled_certificate: None,
    }
}
fn node(authority: &pki::Authority, uid: &str, hardware: &str) -> Node {
    let (_, csr) = pki::request(uid).unwrap();
    Node {
        uid: uid.into(),
        hardware_sha256: hash(hardware),
        token_sha256: hash(uid),
        certificate: authority.sign_client(uid, &csr, 3600).unwrap(),
        enrollment_id: format!("enroll-{uid}"),
        enrollment_digest: hash(&csr),
        pending: None,
        retired: false,
    }
}
fn enroll(state: &mut State, grant: &Bootstrap, node: &Node) -> Result<(), &'static str> {
    state.register(Change::Enroll {
        grant_id: grant.id.clone(),
        bootstrap_sha256: grant.token_sha256.clone(),
        bootstrap_fingerprint: grant.certificate.fingerprint.clone(),
        node: Box::new(node.clone()),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
}
#[test]
fn bootstrap_is_one_use_and_hardware_cannot_register_as_two_nodes() {
    let d = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&d.path().join("ca"), "ca").unwrap();
    let mut s = State::default();
    let g = grant(&ca, "grant-a", "node-a");
    let n = node(&ca, "node-a", "hardware-a");
    s.register(Change::Bootstrap {
        grant: Box::new(g.clone()),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
    .unwrap();
    enroll(&mut s, &g, &n).unwrap();
    enroll(&mut s, &g, &n).unwrap();
    assert_eq!(s.registry.nodes.len(), 1);
    assert_eq!(
        s.registry.bootstraps[&g.id].enrolled_certificate.as_ref(),
        Some(&n.certificate)
    );
    let mut changed = n.clone();
    changed.enrollment_id = "different-request".into();
    assert_eq!(
        enroll(&mut s, &g, &changed),
        Err("bootstrap_already_consumed")
    );
    let other = grant(&ca, "grant-b", "node-b");
    s.register(Change::Bootstrap {
        grant: Box::new(other.clone()),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
    .unwrap();
    assert_eq!(
        enroll(&mut s, &other, &node(&ca, "node-b", "hardware-a")),
        Err("hardware_already_registered")
    );
    let restored: State = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(restored.registry, s.registry);
}
#[test]
fn pending_renewal_overlaps_then_retires_old_leaf_without_changing_node_token() {
    let d = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&d.path().join("ca"), "ca").unwrap();
    let mut s = State::default();
    let g = grant(&ca, "grant-a", "node-a");
    let n = node(&ca, "node-a", "hardware-a");
    s.register(Change::Bootstrap {
        grant: Box::new(g.clone()),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
    .unwrap();
    enroll(&mut s, &g, &n).unwrap();
    let (_, csr) = pki::request("node-a").unwrap();
    let renewed = ca.sign_client("node-a", &csr, 3600).unwrap();
    s.register(Change::Renew {
        node_uid: n.uid.clone(),
        pending: Box::new(PendingRenewal {
            id: "rotation".into(),
            csr_sha256: hash(&csr),
            certificate: renewed.clone(),
        }),
        current_fingerprint: n.certificate.fingerprint.clone(),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
    .unwrap();
    assert_eq!(s.identities.bindings["node:node-a"].fingerprints.len(), 2);
    s.register(Change::Activate {
        node_uid: n.uid.clone(),
        request_id: "rotation".into(),
        fingerprint: renewed.fingerprint.clone(),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
    .unwrap();
    assert_eq!(s.registry.nodes["node-a"].token_sha256, n.token_sha256);
    assert_eq!(
        s.identities.bindings["node:node-a"].fingerprints,
        [renewed.fingerprint].into_iter().collect()
    );
}
#[test]
fn session_and_certificate_fencing_are_checked_at_state_application_time() {
    let d = tempfile::tempdir().unwrap();
    let ca = pki::create_authority(&d.path().join("ca"), "ca").unwrap();
    let mut s = State::default();
    let g = grant(&ca, "grant-a", "node-a");
    let n = node(&ca, "node-a", "hardware-a");
    s.register(Change::Bootstrap {
        grant: Box::new(g.clone()),
        now_ms: converyn_runtime::executors::host::now_ms(),
    })
    .unwrap();
    enroll(&mut s, &g, &n).unwrap();
    let observe = |session: &str, at| Command::ObserveNode {
        diagnostic: None,
        inventory: None,
        resources: None,
        inventory_checked_ms: None,
        capabilities: Vec::new(),
        proof: None,
        node_uid: n.uid.clone(),
        hardware_sha256: Some(n.hardware_sha256.clone()),
        boot_id: "boot".into(),
        session_id: session.into(),
        observed_ms: at,
    };
    assert!(s.apply(observe("session-a", 1000)).ok);
    let proof = Proof {
        principal: "node:node-a".into(),
        fingerprint: n.certificate.fingerprint.clone(),
        token_sha256: n.token_sha256.clone(),
        hardware_sha256: Some(n.hardware_sha256.clone()),
        session: Some("session-a".into()),
    };
    assert!(s.identity_current("node-a", &proof, false));
    assert_eq!(
        s.apply(observe("session-b", 2000)).code,
        "node_session_still_active"
    );
    assert!(s.apply(observe("session-b", 32000)).ok);
    assert!(!s.identity_current("node-a", &proof, false));
    let mut current = proof.clone();
    current.session = Some("session-b".into());
    assert!(s.identity_current("node-a", &current, false));
    let revision = s.identities.bindings["node:node-a"].revision;
    s.identities
        .change(converyn_core::identities::identity::Change {
            principal: "node:node-a".into(),
            expected_revision: revision,
            fingerprints: Default::default(),
            evidence: "revoke while an old request is queued".into(),
        })
        .unwrap();
    let reply = s.apply(Command::Claim {
        input_sha256: None,
        proof: Some(current),
        authorized_ms: None,
        run_id: "unused".into(),
        work_id: "unused".into(),
        node_uid: n.uid,
        grant: "old-grant".into(),
        boot_id: "boot".into(),
        session_id: "session-b".into(),
    });
    assert_eq!(reply.code, "node_identity_fenced");
}
