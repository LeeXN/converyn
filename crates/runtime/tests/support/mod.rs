#![allow(dead_code)]
use converyn_core::{
    identities::{certificate::Signed, registration::Node},
    model::domain::{NodeGroup, Plan},
    orchestration::machine::State,
};
use std::sync::Arc;
/// Pure state-machine fixtures explicitly establish enrollment and platform scope.
/// Transport identity tests use real certificates in their own isolated environments.
pub fn enroll(state: &mut State, uid: &str) {
    state.registry.nodes.entry(uid.into()).or_insert_with(|| {
        Arc::new(Node {
            uid: uid.into(),
            hardware_sha256: converyn_core::model::domain::digest(&uid),
            token_sha256: converyn_core::model::domain::digest(&("token", uid)),
            certificate: Signed {
                certificate_pem: String::new(),
                fingerprint: "f".repeat(64),
                expires_ms: u64::MAX,
            },
            enrollment_id: format!("enroll-{uid}"),
            enrollment_digest: "e".repeat(64),
            pending: None,
            retired: false,
        })
    });
}
pub fn scope(state: &mut State, plan: &Plan) {
    if let Some(hosts) = &plan.input.hosts {
        for uid in &hosts.node_uids {
            enroll(state, uid);
        }
        state
            .node_groups
            .entry(hosts.node_group_uid.clone())
            .or_insert_with(|| NodeGroup {
                uid: hosts.node_group_uid.clone(),
                name: hosts.node_group_uid.clone(),
                revision: hosts.revision,
                node_uids: hosts.node_uids.iter().cloned().collect(),
            });
    }
}
pub fn state() -> State {
    let mut state = State::default();
    let ids = [
        "node",
        "node-a",
        "node-b",
        "node-c",
        "node1",
        "node2",
        "a",
        "b",
        "c",
        "second",
        "test-node",
        "master",
        "worker",
    ];
    for id in ids {
        enroll(&mut state, id);
    }
    for uid in ["cluster", "cluster-lab", "hosts", "another-cluster"] {
        state.node_groups.insert(
            uid.into(),
            NodeGroup {
                uid: uid.into(),
                name: uid.into(),
                revision: 1,
                node_uids: ids.map(String::from).into(),
            },
        );
    }
    let ids: std::collections::BTreeSet<_> = (0..300).map(|i| format!("node-{i}")).collect();
    for id in &ids {
        enroll(&mut state, id);
    }
    state.node_groups.insert(
        "business".into(),
        NodeGroup {
            uid: "business".into(),
            name: "Business".into(),
            revision: 1,
            node_uids: ids,
        },
    );
    state
}
