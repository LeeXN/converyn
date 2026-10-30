mod support;
use converyn_core::model::domain::PlanInput;
use converyn_core::orchestration::machine::{Command, State, Status};
fn plan(hash: &str) -> Box<converyn_core::model::domain::Plan> {
    let v = serde_json::json!({"owner_uid":"app","parameters":{},"batch_size":1,"playbook":{"api_version":"converyn/v1","name":"release","steps":[{"id":"script","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":hash.repeat(64),"timeout_seconds":10},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}],"groups":{"worker":["node"]}},"hosts":{"node_group_uid":"cluster","revision":1,"node_uids":["node"]}});
    Box::new(
        serde_json::from_value::<PlanInput>(v)
            .unwrap()
            .compile()
            .unwrap(),
    )
}
fn reconcile(s: &mut State) {
    assert!(
        s.apply(Command::ReconcileDeployment { uid: "app".into() })
            .ok
    );
}
fn claim(s: &mut State, id: &str) {
    let work = s.runs[id].plan.work[0].clone();
    assert!(
        s.apply(Command::Claim {
            input_sha256: None,
            proof: None,
            authorized_ms: None,
            run_id: id.into(),
            work_id: work.id.clone(),
            node_uid: "node".into(),
            grant: "grant".into(),
            boot_id: "boot".into(),
            session_id: "session".into()
        })
        .ok
    );
    assert!(
        s.apply(Command::Report {
            proof: None,
            receipt: None,
            run_id: id.into(),
            work_id: work.id,
            node_uid: "node".into(),
            grant: "grant".into(),
            sequence: 1,
            status: Status::Running
        })
        .ok
    );
}
fn report(s: &mut State, id: &str, status: Status) {
    let work = s.runs[id].plan.work[0].clone();
    assert!(
        s.apply(Command::Report {
            proof: None,
            receipt: None,
            run_id: id.into(),
            work_id: work.id,
            node_uid: "node".into(),
            grant: "grant".into(),
            sequence: 2,
            status
        })
        .ok
    );
}
#[test]
fn generation_is_not_completion_and_failure_never_auto_replays() {
    let mut s = crate::support::state();
    assert!(
        s.apply(Command::ApplyDeployment {
            uid: "app".into(),
            plan: plan("a")
        })
        .ok
    );
    assert_eq!(s.deployments["app"].generation, 1);
    assert_eq!(s.deployments["app"].applied_generation, None);
    reconcile(&mut s);
    let id = s.deployments["app"].active_run.clone().unwrap();
    claim(&mut s, &id);
    report(&mut s, &id, Status::Failed);
    reconcile(&mut s);
    for _ in 0..3 {
        s.apply(Command::ApplyDeployment {
            uid: "app".into(),
            plan: plan("a"),
        });
        reconcile(&mut s);
    }
    assert_eq!(s.runs.len(), 1);
    assert_eq!(s.deployments["app"].applied_generation, None);
    assert_eq!(s.deployments["app"].condition, "failed");
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: plan("b"),
    });
    reconcile(&mut s);
    let id = s.deployments["app"].active_run.clone().unwrap();
    claim(&mut s, &id);
    report(&mut s, &id, Status::Succeeded);
    reconcile(&mut s);
    assert_eq!(s.deployments["app"].applied_generation, Some(2));
}
#[test]
fn desired_change_cannot_bypass_uncertain_work_after_restart() {
    let mut s = crate::support::state();
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: plan("a"),
    });
    reconcile(&mut s);
    let id = s.deployments["app"].active_run.clone().unwrap();
    claim(&mut s, &id);
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: plan("b"),
    });
    report(&mut s, &id, Status::NeedsAttention);
    let mut s: State = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    reconcile(&mut s);
    assert_eq!(s.runs.len(), 1);
    assert_eq!(s.deployments["app"].condition, "needs_attention");
    assert_eq!(s.deployments["app"].applied_generation, None);
    let r = &s.runs[&id];
    s.apply(Command::Resolve {
        run_id: id.clone(),
        work_id: r.plan.work[0].id.clone(),
        expected_revision: r.revision,
        status: Status::Canceled,
        actor: "operator".into(),
        evidence: "explicit test stop evidence".into(),
    });
    reconcile(&mut s);
    assert_eq!(s.runs.len(), 2);
    assert_eq!(s.deployments["app"].last_started_generation, 2);
}

#[test]
fn terminal_evidence_is_bound_to_intent_and_cannot_change_on_retry() {
    let mut s = crate::support::state();
    let p = plan("a");
    let work = p.work[0].clone();
    assert!(
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: "r".into(),
            idempotency_key: "r".into(),
            plan: p
        })
        .ok
    );
    claim(&mut s, "r");
    let mut receipt = converyn_runtime::executors::runner::Receipt {
        outputs: Default::default(),
        diagnostic: None,
        execution: None,
        logs: None,
        native: None,
        operation_id: format!("r/{}", work.id),
        script_sha256: "a".repeat(64),
        timeout_seconds: 10,
        outcome: converyn_runtime::executors::runner::Outcome::Failed,
        exit_code: Some(42),
        boot_id: "boot".into(),
    };
    let cmd = |r: converyn_runtime::executors::runner::Receipt| Command::Report {
        proof: None,
        run_id: "r".into(),
        work_id: work.id.clone(),
        node_uid: "node".into(),
        grant: "grant".into(),
        sequence: 2,
        status: Status::Succeeded,
        receipt: Some(r),
    };
    assert!(!s.apply(cmd(receipt.clone())).ok);
    receipt.outcome = converyn_runtime::executors::runner::Outcome::Succeeded;
    receipt.exit_code = Some(0);
    assert!(s.apply(cmd(receipt.clone())).ok);
    let revision = s.runs["r"].revision;
    assert!(s.apply(cmd(receipt.clone())).ok);
    assert_eq!(s.runs["r"].revision, revision);
    receipt.boot_id = "different-boot".into();
    assert!(!s.apply(cmd(receipt)).ok);
    assert_eq!(
        s.runs["r"].attempts[&work.id]
            .receipt
            .as_ref()
            .unwrap()
            .exit_code,
        Some(0)
    );
}

#[test]
fn permanently_rejected_admission_does_not_spin_until_explicit_resubmit() {
    let mut s = crate::support::state();
    s.apply(Command::CreateRun {
        output_key: None,
        run_id: "old".into(),
        idempotency_key: "old".into(),
        plan: plan("a"),
    });
    claim(&mut s, "old");
    report(&mut s, "old", Status::Succeeded);
    let input = plan("b").input.clone();
    s.node_groups
        .get_mut("cluster")
        .unwrap()
        .node_uids
        .remove("node");
    let p = Box::new(input.compile().unwrap());
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: p.clone(),
    });
    reconcile(&mut s);
    assert_eq!(s.deployments["app"].condition, "scope_invalid");
    assert!(!s.deployments["app"].admission_pending());
    assert_eq!(s.runs.len(), 1);
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: p,
    });
    assert!(s.deployments["app"].admission_pending());
    assert_eq!(s.runs.len(), 1);
}

#[test]
fn native_success_requires_matching_observed_state_not_script_exit_code() {
    use converyn_core::model::domain::Operation;
    use converyn_runtime::executors::native::{Evidence, FileState};
    use converyn_runtime::executors::runner::{Outcome, Receipt};
    let mut input = plan("a").input;
    input.playbook.steps[0].operation = Operation::File {
        path: "/opt/example/config".into(),
        sha256: "a".repeat(64),
        mode: 0o640,
    };
    let plan = Box::new(input.compile().unwrap());
    let operation = plan.work[0].operation.clone();
    let mut s = crate::support::state();
    assert!(
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: "native".into(),
            idempotency_key: "native".into(),
            plan
        })
        .ok
    );
    claim(&mut s, "native");
    let work = s.runs["native"].plan.work[0].id.clone();
    let mut receipt = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        execution: None,
        logs: None,
        native: Some(Box::new(Evidence {
            plugin: None,
            operation,
            before: None,
            after: None,
            service_before: None,
            service_after: None,
            changed: true,
            error: None,
            artifact_size: None,
            package: None,

            reboot: None,
            health: None,
        })),
        operation_id: format!("native/{work}"),
        script_sha256: String::new(),
        timeout_seconds: 0,
        exit_code: None,
        boot_id: "boot".into(),
        outcome: Outcome::Succeeded,
    };
    let report = |r| Command::Report {
        proof: None,
        receipt: Some(r),
        run_id: "native".into(),
        work_id: work.clone(),
        node_uid: "node".into(),
        grant: "grant".into(),
        sequence: 2,
        status: Status::Succeeded,
    };
    assert!(!s.apply(report(receipt.clone())).ok);
    receipt.native.as_mut().unwrap().after = Some(FileState {
        bytes: 0,
        sha256: "a".repeat(64),
        mode: 0o600,
    });
    assert!(!s.apply(report(receipt.clone())).ok);
    receipt
        .native
        .as_mut()
        .unwrap()
        .after
        .as_mut()
        .unwrap()
        .mode = 0o640;
    assert!(s.apply(report(receipt.clone())).ok);
    assert!(s.apply(report(receipt)).ok);
}
