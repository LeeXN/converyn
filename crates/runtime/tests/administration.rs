mod support;
use converyn_core::{
    model::domain::{NodeGroup, PlanInput},
    orchestration::{
        administration::{Change, MaintenanceWindow},
        machine::{Command, State, Status},
    },
};
use serde_json::json;
fn input() -> PlanInput {
    serde_json::from_value(json!({"hosts":{"node_group_uid":"cluster","revision":1,"node_uids":["node"]},"inventory":{"nodes":[{"uid":"node","address":"192.0.2.1","hostname":"node"}]},"owner_uid":"app","batch_size":1,"playbook":{"api_version":"converyn/v1","name":"config","steps":[{"id":"file","phase":"rollout","scope":"per_node","hosts":"all","operation":{"kind":"file","path":"/opt/app/config","sha256":"a".repeat(64),"mode":416}}]}})).unwrap()
}
fn create(s: &mut State, id: &str, input: PlanInput) {
    let reply = s.apply(Command::CreateRun {
        output_key: None,
        run_id: id.into(),
        idempotency_key: id.into(),
        plan: Box::new(input.compile().unwrap()),
    });
    assert!(reply.ok, "{}", reply.code);
}
#[test]
fn node_group_updates_preserve_snapshots_and_protect_occupied_members() {
    let mut s = support::state();
    create(&mut s, "active", input());
    let original = s.runs["active"].plan.clone();
    let mut group = s.node_groups["cluster"].clone();
    group.name = "Production".into();
    group.revision += 1;
    group.node_uids.remove("second");
    s.administer(Change::UpdateNodeGroup {
        group: group.clone(),
        expected_revision: 1,
    })
    .unwrap();
    assert_eq!(s.runs["active"].plan, original);
    group.revision += 1;
    group.node_uids.remove("node");
    assert_eq!(
        s.administer(Change::UpdateNodeGroup {
            group: group.clone(),
            expected_revision: 2
        }),
        Err("node_group_members_in_use")
    );
    let r = s.runs["active"].revision;
    assert!(
        s.apply(Command::PauseRun {
            run_id: "active".into(),
            paused: true,
            expected_revision: r
        })
        .ok
    );
    assert_eq!(
        s.administer(Change::UpdateNodeGroup {
            group,
            expected_revision: 2
        }),
        Err("node_group_members_in_use")
    );
}
#[test]
fn empty_overlapping_groups_and_retired_uids() {
    let mut s = support::state();
    let group = NodeGroup {
        uid: "new".into(),
        name: "QA".into(),
        revision: 1,
        node_uids: Default::default(),
    };
    s.administer(Change::CreateNodeGroup {
        group: group.clone(),
        expected_revision: 0,
    })
    .unwrap();
    let mut next = group.clone();
    next.revision = 2;
    next.node_uids.insert("node".into());
    s.administer(Change::UpdateNodeGroup {
        group: next,
        expected_revision: 1,
    })
    .unwrap();
    assert!(s.node_groups["cluster"].node_uids.contains("node"));
    s.administer(Change::DeleteNodeGroup {
        uid: "new".into(),
        expected_revision: 2,
        reason: "remove unused scope".into(),
    })
    .unwrap();
    assert_eq!(
        s.administer(Change::CreateNodeGroup {
            group,
            expected_revision: 0
        }),
        Err("node_group_retired")
    );
    assert!(s.registry.nodes.contains_key("node"));
}
#[test]
fn ungrouped_registered_nodes_support_maintenance_and_restore_quarantine() {
    let mut s = support::state();
    support::enroll(&mut s, "ungrouped");
    s.administer(Change::Maintenance {
        node_uid: "ungrouped".into(),
        enabled: true,
    })
    .unwrap();
    s.administer(Change::MaintenanceWindow {
        window: MaintenanceWindow {
            uid: "window".into(),
            revision: 1,
            node_uids: ["ungrouped".into()].into(),
            start_ms: 100,
            end_ms: 200,
            enabled: true,
        },
        expected_revision: 0,
        evidence: "planned".into(),
    })
    .unwrap();
    assert_eq!(
        s.administration.window_until("ungrouped", Some(100)),
        Some(200)
    );
    assert_eq!(s.administration.window_until("ungrouped", Some(200)), None);
    s.administration.maintenance.clear();
    assert!(
        converyn_runtime::storage::backup::quarantine(&s)
            .administration
            .maintenance
            .contains("ungrouped")
    );
    let db = rusqlite::Connection::open_in_memory().unwrap();
    converyn_runtime::storage::state_rows::create(&db).unwrap();
    converyn_runtime::storage::state_rows::save(&db, &State::default(), &s).unwrap();
    assert_eq!(converyn_runtime::storage::state_rows::load(&db).unwrap(), s);
}
#[test]
fn ownership_uses_node_occupancy_across_groups() {
    let mut s = support::state();
    create(&mut s, "first", input());
    let resource = "node/file//opt/app/config";
    let key = s.resource_owners.keys().next().unwrap().clone();
    assert!(key.starts_with("node/"), "{resource}");
    let revision = s.administration.owner_revisions[&key];
    let change = Change::Ownership {
        resource: key,
        expected_owner: "app".into(),
        expected_revision: revision,
        new_owner: Some("other".into()),
        evidence: "handover".into(),
    };
    assert_eq!(s.administer(change.clone()), Err("node_busy"));
    assert!(
        s.apply(Command::Cancel {
            run_id: "first".into()
        })
        .ok
    );
    assert_eq!(s.runs["first"].status, Status::Canceled);
    s.administer(change).unwrap();
}
#[test]
fn registered_members_and_explicit_actions_are_required() {
    let mut s = support::state();
    let group = NodeGroup {
        uid: "unknown".into(),
        name: "Unknown".into(),
        revision: 1,
        node_uids: ["missing".into()].into(),
    };
    assert_eq!(
        s.administer(Change::CreateNodeGroup {
            group,
            expected_revision: 0
        }),
        Err("node_group_member_not_enrolled")
    );
    assert!(
        serde_json::from_value::<Change>(
            json!({"action":"cluster","snapshot":{},"expected_revision":0})
        )
        .is_err()
    );
    assert!(serde_json::from_value::<PlanInput>(json!({"cluster":{}})).is_err());
}
