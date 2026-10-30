mod support;
use converyn_core::model::domain::PlanInput;
use converyn_core::orchestration::machine::Command;
use converyn_runtime::TypeConfig;
use converyn_runtime::storage::store::Store;
use openraft::{
    CommittedLeaderId, Entry, EntryPayload, LogId, RaftSnapshotBuilder, StorageError, Vote,
    storage::{RaftLogStorage, RaftStateMachine},
    testing::{StoreBuilder, Suite},
};

struct Builder;
impl StoreBuilder<TypeConfig, Store, Store, tempfile::TempDir> for Builder {
    async fn build(&self) -> Result<(tempfile::TempDir, Store, Store), StorageError<u64>> {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), 0).unwrap();
        Ok((dir, store.clone(), store))
    }
}

#[test]
fn official_openraft_storage_suite() {
    Suite::test_all(Builder).unwrap();
}

#[tokio::test]
async fn votes_state_and_idempotency_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let plan = serde_yaml::from_str::<PlanInput>(include_str!(
        "../../../scripts/tests/fixtures/plan.yaml"
    ))
    .unwrap()
    .compile()
    .unwrap();
    let command = Command::CreateRun {
        output_key: None,
        run_id: "run-1".into(),
        idempotency_key: "ci-1".into(),
        plan: Box::new(plan),
    };
    let mut store = Store::open(dir.path(), 1).unwrap();
    store
        .restore_business_state(support::state(), "test-fixture")
        .await
        .unwrap();
    store.bind_cluster("cluster-one").await.unwrap();
    store.save_vote(&Vote::new(3, 1)).await.unwrap();
    let id = LogId::new(CommittedLeaderId::new(3, 1), 7);
    assert!(
        store
            .apply([Entry {
                log_id: id,
                payload: EntryPayload::Normal(command.clone())
            }])
            .await
            .unwrap()[0]
            .ok
    );
    drop(store);
    assert!(Store::open(dir.path(), 2).is_err());
    let mut store = Store::open(dir.path(), 1).unwrap();
    assert!(store.bind_cluster("wrong-cluster").await.is_err());
    assert_eq!(store.read_vote().await.unwrap(), Some(Vote::new(3, 1)));
    let machine = store.machine().await.unwrap();
    assert_eq!(machine.last_applied, Some(id));
    assert_eq!(machine.state.runs.len(), 1);
    let response = store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(3, 1), 8),
            payload: EntryPayload::Normal(command),
        }])
        .await
        .unwrap();
    assert!(response[0].ok);
    assert_eq!(store.machine().await.unwrap().state.runs.len(), 1);
}

#[tokio::test]
async fn partial_or_corrupt_snapshot_never_replaces_previous_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path(), 1).unwrap();
    store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), 0),
            payload: EntryPayload::Blank,
        }])
        .await
        .unwrap();
    let before = store.machine().await.unwrap();
    let snapshot = store.build_snapshot().await.unwrap();
    let mut corrupt = converyn_runtime::storage::snapshot_file::SnapshotFile::create(
        &dir.path().join("corrupt.snapshot"),
    )
    .await
    .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut corrupt, b"{partial")
        .await
        .unwrap();
    assert!(
        store
            .install_snapshot(&snapshot.meta, Box::new(corrupt))
            .await
            .is_err()
    );
    assert_eq!(
        store.machine().await.unwrap().last_applied,
        before.last_applied
    );
    drop(store);
    let mut store = Store::open(dir.path(), 1).unwrap();
    assert!(store.get_current_snapshot().await.unwrap().is_some());
    assert_eq!(
        store.machine().await.unwrap().last_applied,
        before.last_applied
    );
}

#[test]
fn only_one_process_can_own_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let _store = Store::open(dir.path(), 1).unwrap();
    assert!(Store::open(dir.path(), 1).is_err());
}

#[test]
fn incompatible_schemas_are_rejected_without_changing_old_data() {
    use sha2::{Digest, Sha256};
    for version in [1, 8, 16, 999] {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("state.sqlite3");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);")
            .unwrap();
        db.execute(
            "INSERT INTO metadata VALUES('schema_version',?1)",
            [serde_json::to_vec(&version).unwrap()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO metadata VALUES('untouched',?1)",
            [b"old immutable evidence".as_slice()],
        )
        .unwrap();
        drop(db);
        let before = Sha256::digest(std::fs::read(&path).unwrap());
        assert!(Store::open(d.path(), 7).is_err());
        assert_eq!(before, Sha256::digest(std::fs::read(&path).unwrap()));
        assert!(!d.path().join("store.lock").exists());
    }
}

#[tokio::test]
async fn heartbeat_only_writes_its_object_and_restart_preserves_history() {
    use converyn_runtime::storage::store::Machine;
    let d = tempfile::tempdir().unwrap();
    let plan = serde_yaml::from_str::<PlanInput>(include_str!(
        "../../../scripts/tests/fixtures/plan.yaml"
    ))
    .unwrap()
    .compile()
    .unwrap();
    let node = plan.input.hosts.as_ref().unwrap().node_uids[0].clone();
    let mut old = Machine {
        state: support::state(),
        ..Machine::default()
    };
    assert!(
        old.state
            .apply(Command::CreateRun {
                output_key: None,
                run_id: "history".into(),
                idempotency_key: "history".into(),
                plan: Box::new(plan)
            })
            .ok
    );
    old.last_applied = Some(LogId::new(CommittedLeaderId::new(1, 1), 1));
    let mut store = Store::open(d.path(), 1).unwrap();
    store
        .restore_business_state(support::state(), "test-fixture")
        .await
        .unwrap();
    store
        .apply([Entry {
            log_id: old.last_applied.unwrap(),
            payload: EntryPayload::Normal(Command::CreateRun {
                output_key: None,
                run_id: "history".into(),
                idempotency_key: "history".into(),
                plan: Box::new(old.state.runs["history"].plan.as_ref().clone()),
            }),
        }])
        .await
        .unwrap();
    let mut actual = store.machine().await.unwrap().state.clone();
    assert_eq!(actual.cold_audit["commands"].count, 1);
    actual.cold_audit.clear();
    assert_eq!(actual, old.state);
    let db = rusqlite::Connection::open(d.path().join("state.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE changed(collection TEXT); CREATE TRIGGER count_state_insert AFTER INSERT ON state_objects BEGIN INSERT INTO changed VALUES(new.collection); END; CREATE TRIGGER count_state_update AFTER UPDATE ON state_objects BEGIN INSERT INTO changed VALUES(new.collection); END;").unwrap();
    let reply = store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), 2),
            payload: EntryPayload::Normal(Command::ObserveNode {
                diagnostic: None,
                inventory: None,
                resources: None,
                inventory_checked_ms: None,
                capabilities: Vec::new(),
                node_uid: node.clone(),
                boot_id: "boot".into(),
                session_id: "session".into(),
                observed_ms: 1,
                hardware_sha256: Some(converyn_core::model::domain::digest(&node)),
                proof: None,
            }),
        }])
        .await
        .unwrap();
    assert!(reply[0].ok);
    let writes: Vec<String> = db
        .prepare("SELECT collection FROM changed")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(writes, vec!["/cold_audit", "/observations"]);
    let expected = store.machine().await.unwrap().state.clone();
    drop(store);
    let store = Store::open(d.path(), 1).unwrap();
    assert_eq!(store.machine().await.unwrap().state, expected);
}

#[test]
fn committed_views_share_history_and_copy_only_the_modified_attempt() {
    use std::sync::Arc;
    let plan = serde_yaml::from_str::<PlanInput>(include_str!(
        "../../../scripts/tests/fixtures/plan.yaml"
    ))
    .unwrap()
    .compile()
    .unwrap();
    let mut state = crate::support::state();
    assert!(
        state
            .apply(Command::CreateRun {
                output_key: None,
                run_id: "shared".into(),
                idempotency_key: "shared".into(),
                plan: Box::new(plan)
            })
            .ok
    );
    let previous = state.clone();
    assert!(Arc::ptr_eq(&state.runs["shared"], &previous.runs["shared"]));
    let work = state.runs["shared"].plan.work[0].clone();
    let node = work.node_uid.clone();
    assert!(
        state
            .apply(Command::Claim {
                input_sha256: None,
                run_id: "shared".into(),
                work_id: work.id.clone(),
                node_uid: node,
                grant: "g".into(),
                boot_id: "b".into(),
                session_id: "s".into(),
                authorized_ms: None,
                proof: None
            })
            .ok
    );
    assert!(!Arc::ptr_eq(
        &state.runs["shared"],
        &previous.runs["shared"]
    ));
    assert!(Arc::ptr_eq(
        &state.runs["shared"].plan,
        &previous.runs["shared"].plan
    ));
    for (id, attempt) in &state.runs["shared"].attempts {
        assert_eq!(
            Arc::ptr_eq(attempt, &previous.runs["shared"].attempts[id]),
            id != &work.id
        );
    }
    assert_eq!(
        previous.runs["shared"].attempts[&work.id].status,
        converyn_core::orchestration::machine::Status::Pending
    );
}
