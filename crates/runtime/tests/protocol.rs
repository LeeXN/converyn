mod support;
use converyn_core::model::domain::{Phase, Plan, PlanInput, Scope};
use converyn_core::orchestration::machine::{Command, State, Status};

fn plan() -> Plan {
    serde_yaml::from_str::<PlanInput>(include_str!("../../../scripts/tests/fixtures/plan.yaml"))
        .unwrap()
        .compile()
        .unwrap()
}
fn create(
    state: &mut State,
    p: Plan,
    id: &str,
    key: &str,
) -> converyn_core::orchestration::machine::Reply {
    state.apply(Command::CreateRun {
        output_key: None,
        run_id: id.into(),
        idempotency_key: key.into(),
        plan: Box::new(p),
    })
}
fn claim(
    state: &mut State,
    p: &Plan,
    index: usize,
) -> converyn_core::orchestration::machine::Reply {
    let w = &p.work[index];
    state.apply(Command::Claim {
        input_sha256: None,
        proof: None,
        authorized_ms: None,
        run_id: "run-1".into(),
        work_id: w.id.clone(),
        node_uid: w.node_uid.clone(),
        grant: format!("grant-{index}"),
        boot_id: "boot-1".into(),
        session_id: "session-1".into(),
    })
}
fn report(
    state: &mut State,
    p: &Plan,
    index: usize,
    sequence: u64,
    status: Status,
) -> converyn_core::orchestration::machine::Reply {
    let w = &p.work[index];
    state.apply(Command::Report {
        proof: None,
        receipt: None,
        run_id: "run-1".into(),
        work_id: w.id.clone(),
        node_uid: w.node_uid.clone(),
        grant: format!("grant-{index}"),
        sequence,
        status,
    })
}
fn finish(state: &mut State, p: &Plan, index: usize) {
    assert!(claim(state, p, index).ok);
    assert!(report(state, p, index, 1, Status::Running).ok);
    assert!(report(state, p, index, 2, Status::Succeeded).ok);
}

#[test]
fn plan_is_deterministic_and_retains_member_order() {
    let p = plan();
    assert_eq!(p, p.input.clone().compile().unwrap());
    assert_eq!(p.work.len(), 7);
    assert_eq!(
        p.work
            .iter()
            .map(|w| w.node_uid.as_str())
            .collect::<Vec<_>>(),
        vec![
            "node-a", "node-a", "node-a", "node-b", "node-b", "node-c", "node-c"
        ]
    );
    let mut changed = p.clone();
    changed.work[0].node_uid = "node-c".into();
    assert!(changed.verify().is_err());
}

#[test]
fn one_shot_step_is_not_repeated_per_batch() {
    let p = plan();
    assert_eq!(
        p.work.iter().filter(|w| w.step_id == "preflight").count(),
        1
    );
    let mut input = p.input;
    input.playbook.steps[0].phase = Phase::Rollout;
    assert!(
        input
            .compile()
            .unwrap_err()
            .to_string()
            .contains("once_per_run")
    );
}

#[test]
fn explicit_batch_hook_has_one_executor_each_batch() {
    let mut input = plan().input;
    input.playbook.steps[0].phase = Phase::Rollout;
    input.playbook.steps[0].scope = Scope::OncePerBatch;
    let p = input.compile().unwrap();
    assert_eq!(
        p.work.iter().filter(|w| w.step_id == "preflight").count(),
        3
    );
}

#[test]
fn reject_ambiguous_or_empty_targets_and_duplicate_nodes() {
    let p = plan();
    let mut i = p.input.clone();
    i.playbook.steps[0].hosts = Some("all".into());
    assert!(i.compile().is_err());
    let mut i = p.input.clone();
    i.playbook.steps[0].hosts = Some("missing".into());
    assert!(i.compile().is_err());
    let mut i = p.input;
    i.hosts.as_mut().unwrap().node_uids.push("node-a".into());
    assert!(i.compile().is_err());
}

#[test]
fn source_changes_do_not_mutate_compiled_plan() {
    let p = plan();
    let mut i = p.input.clone();
    i.parameters
        .insert("environment".into(), "production".into());
    assert_ne!(p.sha256, i.compile().unwrap().sha256);
    assert_eq!(p.input.parameters["environment"], "lab".into());
}

#[test]
fn accepts_300_nodes_and_rejects_301() {
    let mut i = plan().input;
    i.playbook.steps.remove(0);
    let nodes: Vec<_> = (0..300)
        .map(|n| converyn_core::orchestration::inventory::Host {
            uid: format!("n-{n}"),
            hostname: format!("n-{n}"),
            address: format!("192.0.{}.{}", n / 254, n % 254 + 1),
            values: Default::default(),
        })
        .collect();
    let ids: Vec<_> = nodes.iter().map(|n| n.uid.clone()).collect();
    i.inventory.as_mut().unwrap().nodes = nodes;
    i.inventory.as_mut().unwrap().groups = [("worker".into(), ids.clone())].into();
    i.hosts.as_mut().unwrap().node_uids = ids;
    assert_eq!(i.clone().compile().unwrap().work.len(), 600);
    i.hosts.as_mut().unwrap().node_uids.push("extra".into());
    assert!(i.compile().is_err());
}

#[test]
fn create_is_idempotent_and_cluster_claim_is_atomic() {
    let p = plan();
    let mut s = crate::support::state();
    assert!(create(&mut s, p.clone(), "run-1", "ci-1").ok);
    assert_eq!(
        create(&mut s, p.clone(), "different-id", "ci-1")
            .run_id
            .as_deref(),
        Some("run-1")
    );
    assert_eq!(create(&mut s, p.clone(), "run-2", "ci-2").code, "node_busy");
    let mut input = p.input.clone();
    input.parameters.insert("new".into(), "value".into());
    assert_eq!(
        create(&mut s, input.compile().unwrap(), "run-2", "ci-1").code,
        "idempotency_conflict"
    );
    assert_eq!(s.runs.len(), 1);
}

#[test]
fn future_batches_cannot_be_claimed_even_after_recovery() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    assert_eq!(claim(&mut s, &p, 3).code, "dependency_not_ready");
    finish(&mut s, &p, 0);
    finish(&mut s, &p, 1);
    let mut recovered: State = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(claim(&mut recovered, &p, 3).code, "dependency_not_ready");
    finish(&mut recovered, &p, 2);
    assert!(claim(&mut recovered, &p, 3).ok);
}

#[test]
fn duplicate_events_do_not_advance_and_old_events_do_not_rewind() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    assert!(claim(&mut s, &p, 0).ok);
    let before = s.clone();
    assert!(claim(&mut s, &p, 0).ok);
    assert_eq!(s, before);
    assert_eq!(
        report(&mut s, &p, 0, 2, Status::Succeeded).code,
        "event_out_of_order"
    );
    assert!(report(&mut s, &p, 0, 1, Status::Running).ok);
    assert!(report(&mut s, &p, 0, 2, Status::Succeeded).ok);
    let revision = s.runs["run-1"].revision;
    assert!(report(&mut s, &p, 0, 1, Status::Running).ok);
    assert_eq!(s.runs["run-1"].revision, revision);
    assert_eq!(
        report(&mut s, &p, 0, 2, Status::Failed).code,
        "event_conflict"
    );
}

#[test]
fn cancel_does_not_release_authorized_work_without_stop_evidence() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    claim(&mut s, &p, 0);
    assert!(
        s.apply(Command::Cancel {
            run_id: "run-1".into()
        })
        .ok
    );
    assert_eq!(s.runs["run-1"].status, Status::CancelRequested);
    assert!(s.execution_locks.contains_key("node/node-a"));
    assert!(report(&mut s, &p, 0, 1, Status::Canceled).ok);
    assert_eq!(s.runs["run-1"].status, Status::Canceled);
    assert!(s.execution_locks.is_empty());
}

#[test]
fn uncertainty_survives_restart_and_requires_revision_bound_resolution() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    claim(&mut s, &p, 0);
    let rev = s.runs["run-1"].revision;
    assert!(
        s.apply(Command::MarkUncertain {
            reason: None,
            run_id: "run-1".into(),
            work_id: p.work[0].id.clone(),
            expected_revision: rev
        })
        .ok
    );
    assert_eq!(s.runs["run-1"].status, Status::NeedsAttention);
    let mut s: State = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert!(s.execution_locks.contains_key("node/node-a"));
    let resolution = |revision, evidence: &str| Command::Resolve {
        run_id: "run-1".into(),
        work_id: p.work[0].id.clone(),
        expected_revision: revision,
        status: Status::Failed,
        actor: "test-operator".into(),
        evidence: evidence.into(),
    };
    assert_eq!(
        s.apply(resolution(rev, "stopped")).code,
        "revision_conflict"
    );
    assert_eq!(s.apply(resolution(rev + 1, "")).code, "invalid_resolution");
    assert!(
        s.apply(resolution(
            rev + 1,
            "test executor is stopped; result unavailable"
        ))
        .ok
    );
    assert_eq!(s.runs["run-1"].status, Status::Failed);
    assert_eq!(s.audit.len(), 1);
    assert!(s.execution_locks.is_empty());
}

#[test]
fn persistent_ownership_prevents_sequential_apps_fighting_over_resource() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    for n in 0..p.work.len() {
        finish(&mut s, &p, n);
    }
    assert_eq!(s.runs["run-1"].status, Status::Succeeded);
    let mut input = p.input;
    input.owner_uid = "other-app".into();
    assert_eq!(
        create(&mut s, input.compile().unwrap(), "run-2", "ci-2").code,
        "resource_owner_conflict"
    );
}

#[test]
fn wrong_node_or_grant_cannot_report_completion() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    claim(&mut s, &p, 0);
    for (node, grant, expected) in [
        ("node-b", "grant-0", "wrong_node"),
        ("node-a", "old-grant", "wrong_grant"),
    ] {
        let r = s.apply(Command::Report {
            proof: None,
            receipt: None,
            run_id: "run-1".into(),
            work_id: p.work[0].id.clone(),
            node_uid: node.into(),
            grant: grant.into(),
            sequence: 1,
            status: Status::Succeeded,
        });
        assert_eq!(r.code, expected);
    }
}

#[test]
fn another_cluster_cannot_rebind_a_node_to_bypass_serialization() {
    let p = plan();
    let mut state = crate::support::state();
    assert!(create(&mut state, p.clone(), "run-1", "ci-1").ok);
    let mut input = p.input;
    input.hosts.as_mut().unwrap().node_group_uid = "another-cluster".into();
    assert_eq!(
        create(&mut state, input.compile().unwrap(), "run-2", "ci-2").code,
        "node_busy"
    );
    assert_eq!(state.runs.len(), 1);
}

#[test]
fn executor_cancellation_stops_downstream_work() {
    let p = plan();
    let mut s = crate::support::state();
    create(&mut s, p.clone(), "run-1", "ci-1");
    claim(&mut s, &p, 0);
    assert!(report(&mut s, &p, 0, 1, Status::Canceled).ok);
    assert_eq!(s.runs["run-1"].status, Status::Canceled);
    assert!(s.execution_locks.is_empty());
}

#[test]
fn unused_members_do_not_create_extra_rollout_batches() {
    let mut input = plan().input;
    input
        .inventory
        .as_mut()
        .unwrap()
        .nodes
        .push(converyn_core::orchestration::inventory::Host {
            uid: "unrelated".into(),
            hostname: "unrelated".into(),
            address: "192.0.2.99".into(),
            values: Default::default(),
        });
    input
        .hosts
        .as_mut()
        .unwrap()
        .node_uids
        .push("unrelated".into());
    input.playbook.steps[0].scope = Scope::OncePerBatch;
    input.playbook.steps[0].phase = Phase::Rollout;
    let p = input.compile().unwrap();
    assert_eq!(
        p.work.iter().filter(|w| w.step_id == "preflight").count(),
        3
    );
}
