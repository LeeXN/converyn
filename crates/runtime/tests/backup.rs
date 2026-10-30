mod support;
use converyn_core::model::domain::PlanInput;
use converyn_core::orchestration::machine::{Command, State, Status};
use converyn_runtime::storage::{backup, store::Store};
fn state() -> State {
    let mut state = crate::support::state();
    let plan = serde_yaml::from_str::<PlanInput>(include_str!(
        "../../../scripts/tests/fixtures/plan.yaml"
    ))
    .unwrap()
    .compile()
    .unwrap();
    assert!(
        state
            .apply(Command::CreateRun {
                output_key: None,
                run_id: "active".into(),
                idempotency_key: "active".into(),
                plan: Box::new(plan)
            })
            .ok
    );
    state
}
#[test]
fn recovery_quarantines_work_without_mutating_original_evidence() {
    let original = state();
    let recovered = backup::quarantine(&original);
    assert!(recovered.runs["active"].pause_requested);
    assert_eq!(recovered.runs["active"].status, Status::Paused);
    assert_eq!(
        recovered.administration.maintenance.len(),
        recovered
            .registry
            .nodes
            .values()
            .filter(|n| !n.retired)
            .count()
    );
    assert_eq!(recovered.runs["active"].plan, original.runs["active"].plan);
    assert_eq!(original.runs["active"].status, Status::Pending);
}
#[tokio::test]
async fn sqlite_restore_requires_fencing_and_new_directory_and_verifies_checksum() {
    let d = tempfile::tempdir().unwrap();
    let source = Store::open(&d.path().join("source"), 1).unwrap();
    source.bind_cluster("control").await.unwrap();
    let original = state();
    source
        .restore_business_state(original.clone(), "test")
        .await
        .unwrap();
    let (path, _) = source.backup_carrier().await.unwrap();
    assert_eq!(backup::inspect(&path).unwrap().1, "control");
    let output = d.path().join("restored");
    assert!(backup::restore(&path, &output, 1, false).await.is_err());
    assert!(!output.exists());
    backup::restore(&path, &output, 1, true).await.unwrap();
    let restored = Store::open(&output, 1).unwrap();
    assert_eq!(
        restored.machine().await.unwrap().state,
        backup::quarantine(&original)
    );
    assert!(backup::restore(&path, &output, 1, true).await.is_err());
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    let corrupt = d.path().join("corrupt.cvb");
    std::fs::write(&corrupt, bytes).unwrap();
    assert!(backup::inspect(&corrupt).is_err());
    let json = d.path().join("unsupported.json");
    std::fs::write(&json, b"{}").unwrap();
    assert!(backup::inspect(&json).is_err());
}
