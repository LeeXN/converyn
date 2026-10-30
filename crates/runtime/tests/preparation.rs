mod support;
use converyn_core::model::domain::PlanInput;
use converyn_core::orchestration::machine::{Command, State, Status};
use converyn_core::orchestration::preparation::Change;
use serde_json::json;
fn create(state: &mut State, id: &str) {
    let input:PlanInput=serde_json::from_value(json!({"owner_uid":"app","parameters":{},"batch_size":1,"playbook":{"api_version":"converyn/v1","name":"binary","steps":[{"id":"binary","phase":"rollout","scope":"per_node","operation":{"kind":"binary","path":"/opt/app/main","mode":493,"artifact":{"sha256":"a".repeat(64),"size":10,"locations":[{"source":"private","path":"/binary"}]}},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"a","hostname":"a","address":"192.0.2.1"},{"uid":"b","hostname":"b","address":"192.0.2.2"}],"groups":{"worker":["a","b"]}},"hosts":{"node_group_uid":"cluster","revision":1,"node_uids":["a","b"]}})).unwrap();
    assert!(
        state
            .apply(Command::CreateRun {
                output_key: None,
                run_id: id.into(),
                idempotency_key: id.into(),
                plan: Box::new(input.compile().unwrap())
            })
            .ok
    );
}
fn ready(state: &mut State, id: &str) {
    for w in state.runs[id].plan.work.clone() {
        state
            .prepare(Change::Prepared {
                run_id: id.into(),
                work_id: w.id,
                node_uid: w.node_uid,
                sha256: "a".repeat(64),
                size: 10,
                proof: None,
            })
            .unwrap();
    }
}
#[test]
fn downloads_finish_before_cluster_lock_and_execution_authorization() {
    let mut state = crate::support::state();
    create(&mut state, "run");
    assert_eq!(state.runs["run"].status, Status::Preparing);
    assert!(state.execution_locks.is_empty());
    let w = state.runs["run"].plan.work[0].clone();
    assert_eq!(
        state
            .apply(Command::Claim {
                input_sha256: None,
                run_id: "run".into(),
                work_id: w.id.clone(),
                node_uid: w.node_uid.clone(),
                grant: "grant".into(),
                boot_id: "boot".into(),
                session_id: "session".into(),
                authorized_ms: None,
                proof: None
            })
            .code,
        "not_claimable"
    );
    assert_eq!(
        state.prepare(Change::Admit {
            run_id: "run".into(),
            expected_revision: 1
        }),
        Err("artifacts_not_ready")
    );
    assert!(
        state
            .prepare(Change::Prepared {
                run_id: "run".into(),
                work_id: w.id,
                node_uid: "foreign".into(),
                sha256: "a".repeat(64),
                size: 10,
                proof: None
            })
            .is_err()
    );
    ready(&mut state, "run");
    state
        .prepare(Change::Admit {
            run_id: "run".into(),
            expected_revision: state.runs["run"].revision,
        })
        .unwrap();
    assert_eq!(state.execution_locks["node/a"], "run");
    assert!(!state.runs["run"].preparing);
}
#[test]
fn canceling_waiting_preparation_cannot_release_another_runs_lock() {
    let mut state = crate::support::state();
    create(&mut state, "first");
    create(&mut state, "second");
    ready(&mut state, "first");
    state
        .prepare(Change::Admit {
            run_id: "first".into(),
            expected_revision: state.runs["first"].revision,
        })
        .unwrap();
    assert!(
        state
            .apply(Command::Cancel {
                run_id: "second".into()
            })
            .ok
    );
    assert_eq!(state.runs["second"].status, Status::Canceled);
    assert_eq!(state.execution_locks["node/a"], "first");
}
#[test]
fn pinned_cache_cannot_evict_rollback_bytes_to_fit_another_download() {
    let d = tempfile::tempdir().unwrap();
    let digest = "a".repeat(64);
    std::fs::write(d.path().join(&digest), [1u8; 8]).unwrap();
    converyn_runtime::artifacts::cache::set_pins(d.path(), &[(digest.clone(), 8)].into(), 10)
        .unwrap();
    assert!(converyn_runtime::artifacts::cache::reserve(d.path(), &"b".repeat(64), 8, 10).is_err());
    assert!(d.path().join(&digest).exists());
    converyn_runtime::artifacts::cache::set_pins(d.path(), &Default::default(), 10).unwrap();
    converyn_runtime::artifacts::cache::reserve(d.path(), &"b".repeat(64), 8, 10).unwrap();
    assert!(!d.path().join(digest).exists());
}
