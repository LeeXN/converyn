mod support;
use converyn_core::{
    model::domain::{Plan, digest},
    orchestration::{
        administration::Change as GroupChange,
        machine::{Command, State},
        preparation,
        workflow::{Change, Request, Revision},
    },
};
use serde_json::json;
use std::collections::BTreeMap;
fn source(uid: &str, selector: &str) -> Revision {
    serde_json::from_value(json!({"uid":uid,"name":uid,"inventory":{"nodes":[{"uid":"a","hostname":"a","address":"192.0.2.1"},{"uid":"b","hostname":"b","address":"192.0.2.2"}],"groups":{"database":["b","a"]}},"targets":{"qa":{"node_group_uid":"hosts","hosts":selector}},"batch_size":2,"playbook":{"api_version":"converyn/v1","name":uid,"steps":[{"id":"probe","phase":"rollout","scope":"per_node","hosts":selector,"max_parallel":1,"operation":{"kind":"http_check","url":"https://example.invalid/","expected_status":200,"timeout_seconds":1}}]}})).unwrap()
}
fn register(s: &mut State, r: Revision) -> String {
    let sha = digest(&r);
    s.workflow_change(
        Change::Register {
            workflow: Box::new(r),
            expected_revision: 0,
        },
        &converyn_plugin_api::NoPlugins,
    )
    .unwrap();
    sha
}
fn plan(s: &State, uid: &str, sha: &str) -> (Plan, String) {
    let (p, _, t) = s
        .workflow_plan(
            uid,
            sha,
            "qa",
            &BTreeMap::new(),
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap();
    (p, t)
}
#[test]
fn inventory_order_is_independent_and_added_members_do_not_expand_work() {
    let mut s = support::state();
    let sha = register(&mut s, source("first", "database"));
    let (p, _) = plan(&s, "first", &sha);
    assert_eq!(
        p.work
            .iter()
            .map(|w| w.node_uid.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    let second = register(&mut s, source("second", "all"));
    let (q, _) = plan(&s, "second", &second);
    assert_eq!(
        q.work
            .iter()
            .map(|w| w.node_uid.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let mut group = s.node_groups["hosts"].clone();
    group.revision += 1;
    group.node_uids.insert("extra".into());
    support::enroll(&mut s, "extra");
    s.administer(GroupChange::UpdateNodeGroup {
        group,
        expected_revision: 1,
    })
    .unwrap();
    let (after, _) = plan(&s, "first", &sha);
    assert_eq!(
        p.work.iter().map(|w| &w.node_uid).collect::<Vec<_>>(),
        after.work.iter().map(|w| &w.node_uid).collect::<Vec<_>>()
    );
}
#[test]
fn out_of_scope_registration_is_atomic_and_previews_cannot_be_reused() {
    let mut s = support::state();
    s.node_groups
        .get_mut("hosts")
        .unwrap()
        .node_uids
        .remove("b");
    let before = s.workflows.clone();
    let error = s
        .workflow_change(
            Change::Register {
                workflow: Box::new(source("bad", "all")),
                expected_revision: 0,
            },
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("group=hosts:target=qa") && error.contains("b"),
        "{error}"
    );
    assert_eq!(s.workflows, before);
    s.node_groups
        .get_mut("hosts")
        .unwrap()
        .node_uids
        .insert("b".into());
    let sha = register(&mut s, source("flow", "all"));
    let (p, t) = plan(&s, "flow", &sha);
    s.node_groups.get_mut("hosts").unwrap().revision += 1;
    let request = Request {
        workflow_uid: "flow".into(),
        workflow_revision: sha,
        target: "qa".into(),
        parameters: BTreeMap::new(),
        request_id: "stale".into(),
        plan_sha256: p.sha256,
        target_sha256: t,
    };
    let actor=serde_json::from_value(json!({"uid":"user","name":"User","credential_uid":"credential","fingerprint":"a".repeat(64)})).unwrap();
    let error = s
        .workflow_change(
            Change::Execute {
                request: Box::new(request),
                actor,
                now_ms: 1,
                output_key: None,
            },
            &converyn_plugin_api::NoPlugins,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("preview_stale"), "{error}");
    assert!(s.runs.is_empty());
}
#[test]
fn groups_never_lock_but_overlapping_nodes_do() {
    let mut s = support::state();
    let a = register(&mut s, source("first", "all[0]"));
    let b = register(&mut s, source("second", "all[1]"));
    let (p, _) = plan(&s, "first", &a);
    let (q, _) = plan(&s, "second", &b);
    let create = |s: &mut State, id: &str, p: Plan| {
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: id.into(),
            idempotency_key: id.into(),
            plan: Box::new(p),
        })
    };
    assert!(create(&mut s, "a", p.clone()).ok);
    assert!(create(&mut s, "b", q).ok);
    assert_eq!(s.execution_locks.len(), 2);
    assert!(!s.execution_locks.contains_key("hosts"));
    let mut overlap = p.input;
    overlap.hosts.as_mut().unwrap().node_group_uid = "cluster".into();
    assert_eq!(
        create(&mut s, "overlap", overlap.compile().unwrap()).code,
        "node_busy"
    );
}
#[test]
fn disabled_workflows_still_protect_group_references() {
    let mut s = support::state();
    register(&mut s, source("flow", "all"));
    s.workflows.workflows.get_mut("flow").unwrap().enabled = false;
    assert_eq!(
        s.administer(GroupChange::DeleteNodeGroup {
            uid: "hosts".into(),
            expected_revision: 1,
            reason: "decommission".into()
        }),
        Err("node_group_referenced_by_workflow")
    );
}
#[test]
fn preparation_rechecks_membership_and_allows_unrelated_revision_changes() {
    for remove_required in [false, true] {
        let mut s = support::state();
        let mut r = source("prepare", "all[0]");
        r.playbook.steps[0].operation=serde_json::from_value(json!({"kind":"binary","path":"/opt/example/tool","mode":493,"artifact":{"sha256":"a".repeat(64),"size":10,"locations":[{"source":"source","path":"/tool"}]}})).unwrap();
        let sha = register(&mut s, r);
        let (p, _) = plan(&s, "prepare", &sha);
        assert!(
            s.apply(Command::CreateRun {
                output_key: None,
                run_id: "prepare".into(),
                idempotency_key: "prepare".into(),
                plan: Box::new(p.clone())
            })
            .ok
        );
        assert!(s.runs["prepare"].preparing);
        let mut g = s.node_groups["hosts"].clone();
        g.revision += 1;
        g.node_uids.remove(if remove_required { "a" } else { "b" });
        s.administer(GroupChange::UpdateNodeGroup {
            group: g,
            expected_revision: 1,
        })
        .unwrap();
        let w = &p.work[0];
        s.prepare(preparation::Change::Prepared {
            run_id: "prepare".into(),
            work_id: w.id.clone(),
            node_uid: w.node_uid.clone(),
            sha256: "a".repeat(64),
            size: 10,
            proof: None,
        })
        .unwrap();
        let result = s.prepare(preparation::Change::Admit {
            run_id: "prepare".into(),
            expected_revision: s.runs["prepare"].revision,
        });
        if remove_required {
            assert_eq!(result, Err("scope_invalid"));
            assert!(s.execution_locks.is_empty());
        } else {
            result.unwrap();
            assert!(s.execution_locks.contains_key("node/a"));
        }
    }
}
#[test]
fn joint_approval_depends_on_target_group_bindings_not_inventory_edits() {
    use converyn_core::identities::{
        access::{Binding, Role, Subject},
        personnel::{Credential, Identity},
        registration::Authorization,
    };
    let mut s = support::state();
    s.personnel.identities.insert(
        "maintainer".into(),
        Identity {
            uid: "maintainer".into(),
            name: "Maintainer".into(),
            disabled: false,
            revision: 1,
        },
    );
    s.personnel.credentials.insert(
        "credential".into(),
        Credential {
            uid: "credential".into(),
            identity_uid: "maintainer".into(),
            certificate_pem: String::new(),
            fingerprint: "f".repeat(64),
            token_sha256: "t".repeat(64),
            not_before_ms: 0,
            expires_ms: 1000,
            revoked_ms: None,
            overlap_until_ms: None,
        },
    );
    let binding = |uid: &str, role: Role| Binding {
        uid: uid.into(),
        subject: Subject::Person("maintainer".into()),
        role,
        workflow: if role == Role::Maintainer {
            Some("flow".into())
        } else {
            None
        },
        presets: Default::default(),
    };
    s.personnel
        .access
        .bindings
        .insert("maintain".into(), binding("maintain", Role::Maintainer));
    let auth = Authorization {
        principal: "maintainer".into(),
        fingerprint: "f".repeat(64),
        token_sha256: Some("t".repeat(64)),
        observed_ms: Some(1),
        request_id: None,
    };
    let command = |r: Revision, revision: u64| Command::Workflow {
        change: Change::Register {
            workflow: Box::new(r),
            expected_revision: revision,
        },
    };
    let r = source("flow", "all");
    assert!(!s.access_allows(&auth, &command(r.clone(), 0)));
    s.personnel.access.bindings.insert(
        "platform".into(),
        binding("platform", Role::PlatformOperator),
    );
    assert!(s.access_allows(&auth, &command(r.clone(), 0)));
    assert!(
        s.apply(Command::Authorized {
            authorization: auth.clone(),
            command: Box::new(command(r.clone(), 0))
        })
        .ok
    );
    s.personnel.access.bindings.remove("platform");
    let mut topology = r.clone();
    topology.inventory.as_mut().unwrap().nodes[0].address = "192.0.2.99".into();
    topology
        .inventory
        .as_mut()
        .unwrap()
        .groups
        .get_mut("database")
        .unwrap()
        .reverse();
    assert!(s.access_allows(&auth, &command(topology, 1)));
    let mut different = r.clone();
    different.targets.get_mut("qa").unwrap().node_group_uid = Some("cluster".into());
    assert!(!s.access_allows(&auth, &command(different, 1)));
    let mut renamed = r.clone();
    let t = renamed.targets.remove("qa").unwrap();
    renamed.targets.insert("production".into(), t);
    assert!(!s.access_allows(&auth, &command(renamed, 1)));
    let mut deleted = r.clone();
    deleted.targets.clear();
    assert!(s.access_allows(&auth, &command(deleted.clone(), 1)));
    let removed = s.apply(Command::Authorized {
        authorization: auth.clone(),
        command: Box::new(command(deleted, 1)),
    });
    assert!(removed.ok, "{}", removed.code);
    assert!(!s.access_allows(&auth, &command(r, 2)));
    assert!(
        s.workflow_plan(
            "flow",
            &s.workflows.workflows["flow"].current,
            "qa",
            &BTreeMap::new(),
            &converyn_plugin_api::NoPlugins
        )
        .is_err()
    );
    s.administer(GroupChange::DeleteNodeGroup {
        uid: "hosts".into(),
        expected_revision: 1,
        reason: "last workflow binding removed".into(),
    })
    .unwrap();
}
#[test]
fn target_inventory_hides_other_nodes_but_keeps_declared_empty_groups() {
    let mut r = source("view", "all");
    let inventory = r.inventory.as_mut().unwrap();
    inventory.groups.insert("optional".into(), vec![]);
    inventory.groups.insert("other".into(), vec!["b".into()]);
    let filtered = inventory.restricted(&["a".into()].into());
    assert_eq!(filtered.nodes.len(), 1);
    assert_eq!(filtered.groups["database"], ["a"]);
    assert!(filtered.groups.contains_key("optional"));
    assert!(!filtered.groups.contains_key("other"));
    assert_eq!(inventory.nodes.len(), 2);
}

#[test]
fn selector_order_survives_single_node_batches() {
    let mut state = support::state();
    let mut revision = source("serial-order", "database");
    revision.batch_size = 1;
    let sha = register(&mut state, revision);
    let (compiled, _) = plan(&state, "serial-order", &sha);
    assert_eq!(
        compiled
            .work
            .iter()
            .map(|w| (w.node_uid.as_str(), w.batch))
            .collect::<Vec<_>>(),
        [("b", Some(0)), ("a", Some(1))]
    );
    compiled.verify().unwrap();
}
