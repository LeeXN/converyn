mod support;
use converyn_core::model::domain::PlanInput;
use converyn_core::orchestration::machine::{Command, State, Status};
fn plan() -> converyn_core::model::domain::Plan {
    let input = serde_json::json!({"owner_uid":"app","parameters":{},"batch_size":1,"playbook":{"api_version":"converyn/v1","name":"desired","steps":[{"id":"hook","phase":"prepare","scope":"once_per_run","operation":{"kind":"exec","script_sha256":"b".repeat(64),"timeout_seconds":10},"hosts":"coordinator"},{"id":"file","phase":"rollout","scope":"per_node","operation":{"kind":"file","path":"/opt/app/config","sha256":"a".repeat(64),"mode":416},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"node-a","hostname":"node-a","address":"192.0.2.1"},{"uid":"node-b","hostname":"node-b","address":"192.0.2.2"}],"groups":{"worker":["node-a","node-b"],"coordinator":["node-a"]}},"hosts":{"node_group_uid":"cluster","revision":1,"node_uids":["node-a","node-b"]}});
    serde_json::from_value::<PlanInput>(input)
        .unwrap()
        .compile()
        .unwrap()
}
fn finish(s: &mut State, id: &str) {
    let work = s.runs[id].plan.work.clone();
    for w in work {
        assert!(
            s.apply(Command::Claim {
                input_sha256: None,
                proof: None,
                authorized_ms: None,
                run_id: id.into(),
                work_id: w.id.clone(),
                node_uid: w.node_uid.clone(),
                grant: "grant".into(),
                boot_id: "boot".into(),
                session_id: "session".into()
            })
            .ok
        );
        for (sequence, status) in [(1, Status::Running), (2, Status::Succeeded)] {
            assert!(
                s.apply(Command::Report {
                    proof: None,
                    run_id: id.into(),
                    work_id: w.id.clone(),
                    node_uid: w.node_uid.clone(),
                    grant: "grant".into(),
                    sequence,
                    status,
                    receipt: None
                })
                .ok
            );
        }
    }
}
#[test]
fn periodic_native_checks_preserve_batches_without_replaying_shell_hooks() {
    let mut s = crate::support::state();
    assert!(
        s.apply(Command::ApplyDeployment {
            uid: "app".into(),
            plan: Box::new(plan())
        })
        .ok
    );
    s.apply(Command::ReconcileDeployment { uid: "app".into() });
    let rollout = s.deployments["app"].active_run.clone().unwrap();
    finish(&mut s, &rollout);
    s.apply(Command::ReconcileDeployment { uid: "app".into() });
    s.configure_drift("app", Some(30)).unwrap();
    assert!(s.drift_due("app", 1000));
    s.reconcile_drift("app", 1000, 1).unwrap();
    let check = s.drift["app"].active_run.clone().unwrap();
    assert_eq!(s.runs[&check].plan.work.len(), 2);
    assert!(s.runs[&check].plan.work.iter().all(|w| w.step_id == "file"));
    assert_ne!(
        s.runs[&check].plan.work[0].wave,
        s.runs[&check].plan.work[1].wave
    );
    assert!(!s.drift_due("app", 40000));
    finish(&mut s, &check);
    assert!(s.drift_due("app", 40000));
    s.reconcile_drift("app", 40000, 1).unwrap();
    assert_eq!(s.drift["app"].observed_generation, Some(1));
    assert!(!s.drift_due("app", 100));
    assert!(s.drift_due("app", 40000));
    s.reconcile_drift("app", 40000, 1).unwrap();
    assert_ne!(s.drift["app"].active_run.as_deref(), Some(check.as_str()));
    assert_eq!(s.runs[&rollout].plan.work.len(), 3);
    assert_eq!(s.runs[&check].status, Status::Succeeded);
}
#[test]
fn checks_cannot_run_before_rollout_success_or_for_shell_only_plan() {
    let mut s = crate::support::state();
    let mut p = plan().input;
    p.playbook.steps.retain(|s| s.id == "hook");
    let p = p.compile().unwrap();
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: Box::new(p),
    });
    assert!(s.configure_drift("app", Some(30)).is_err());
    s.apply(Command::ApplyDeployment {
        uid: "app".into(),
        plan: Box::new(plan()),
    });
    s.configure_drift("app", Some(30)).unwrap();
    assert!(!s.drift_due("app", 100000));
    assert!(s.reconcile_drift("app", 100000, 1).is_err());
}
