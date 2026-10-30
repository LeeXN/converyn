mod support;
use converyn_core::model::domain::PlanInput;
use converyn_core::orchestration::history::{Change, Kind};
use converyn_core::orchestration::machine::Command;
use converyn_runtime::storage::store::Store;
use openraft::{
    CommittedLeaderId, Entry, EntryPayload, LogId, RaftSnapshotBuilder, storage::RaftStateMachine,
};
async fn apply(
    store: &mut Store,
    index: u64,
    command: Command,
) -> converyn_core::orchestration::machine::Reply {
    store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), index),
            payload: EntryPayload::Normal(command),
        }])
        .await
        .unwrap()
        .remove(0)
}
fn plan() -> converyn_core::model::domain::Plan {
    serde_yaml::from_str::<PlanInput>(include_str!("../../../scripts/tests/fixtures/plan.yaml"))
        .unwrap()
        .compile()
        .unwrap()
}

#[tokio::test]
async fn explicit_clear_removes_hot_and_cold_runs_without_replaying_retired_ids() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(&directory.path().join("source"), 1).unwrap();
    store
        .restore_business_state(support::state(), "test-fixture")
        .await
        .unwrap();
    store.bind_cluster("history-clear").await.unwrap();
    let plan = plan();
    let create = |id: &str, key: &str| Command::CreateRun {
        output_key: None,
        run_id: id.into(),
        idempotency_key: key.into(),
        plan: Box::new(plan.clone()),
    };
    assert!(apply(&mut store, 1, create("run-old", "old")).await.ok);
    let sequence = store.machine().await.unwrap().state.run_sequence;
    let clear = Command::History {
        change: Change::ClearRuns {
            expected_sequence: sequence,
            backup_sha256: "a".repeat(64),
            evidence: "verified operator backup".into(),
        },
    };
    assert!(
        !apply(&mut store, 2, clear.clone()).await.ok,
        "active Run was cleared"
    );
    assert!(
        apply(
            &mut store,
            3,
            Command::Cancel {
                run_id: "run-old".into()
            }
        )
        .await
        .ok
    );
    let revision = store.machine().await.unwrap().state.runs["run-old"].revision;
    assert!(
        apply(
            &mut store,
            4,
            Command::History {
                change: Change::Archive {
                    kind: Kind::Run,
                    uid: "run-old".into(),
                    expected_revision: revision,
                    now_ms: 100,
                }
            }
        )
        .await
        .ok
    );
    let before = store.machine().await.unwrap();
    let idempotency = before.state.idempotency.clone();
    let owners = before.state.resource_owners.clone();
    let clusters = before.state.node_groups.clone();
    assert!(apply(&mut store, 5, clear.clone()).await.ok);
    assert!(store.load_run("run-old").await.unwrap().is_none());
    let after = store.machine().await.unwrap();
    assert!(after.state.runs.is_empty() && after.state.workflows.executions.is_empty());
    assert!(after.state.history.get(Kind::Run, "run-old").is_none());
    assert_eq!(after.state.idempotency, idempotency);
    assert_eq!(after.state.resource_owners, owners);
    assert_eq!(after.state.node_groups, clusters);
    assert_eq!(after.state.run_sequence, sequence);
    assert_eq!(
        apply(&mut store, 6, create("run-old", "another-key"))
            .await
            .code,
        "run_history_cleared"
    );
    let request = converyn_core::orchestration::workflow::Request {
        workflow_uid: "different-target-workflow".into(),
        workflow_revision: "b".repeat(64),
        target: "other-cluster".into(),
        parameters: Default::default(),
        request_id: "old".into(),
        plan_sha256: "c".repeat(64),
        target_sha256: "d".repeat(64),
    };
    let reply = apply(
        &mut store,
        7,
        Command::Workflow {
            change: converyn_core::orchestration::workflow::Change::Execute {
                output_key: None,
                request: Box::new(request),
                actor: converyn_core::orchestration::workflow::Actor {
                    uid: "alice".into(),
                    name: "Alice".into(),
                    fingerprint: "f".repeat(64),
                    credential_uid: "credential".into(),
                },
                now_ms: 200,
            },
        },
    )
    .await;
    assert!(reply.code.contains("run_history_cleared"));
    let db = rusqlite::Connection::open(directory.path().join("source/state.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM cold_history WHERE kind='run'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    drop(db);
    assert!(apply(&mut store, 8, create("new-run", "new-key")).await.ok);
    assert!(
        apply(
            &mut store,
            9,
            Command::Cancel {
                run_id: "new-run".into()
            }
        )
        .await
        .ok
    );
    let revision = store.machine().await.unwrap().state.runs["new-run"].revision;
    assert!(
        apply(
            &mut store,
            10,
            Command::History {
                change: Change::Archive {
                    kind: Kind::Run,
                    uid: "new-run".into(),
                    expected_revision: revision,
                    now_ms: 300
                }
            }
        )
        .await
        .ok
    );
    assert!(
        apply(&mut store, 11, clear).await.ok,
        "original clear retry was not idempotent"
    );
    assert!(
        store.load_run("new-run").await.unwrap().is_some(),
        "retry deleted newer cold history"
    );
    let snapshot = store.build_snapshot().await.unwrap();
    let mut replica = Store::open(&directory.path().join("replica"), 2).unwrap();
    replica
        .install_snapshot(&snapshot.meta, snapshot.snapshot)
        .await
        .unwrap();
    assert!(replica.load_run("run-old").await.unwrap().is_none());
    assert!(replica.load_run("new-run").await.unwrap().is_some());
    assert_eq!(
        replica.machine().await.unwrap().state.retirement_proofs,
        store.machine().await.unwrap().state.retirement_proofs
    );
}
#[tokio::test]
async fn complete_cold_history_survives_snapshot_backup_restore_and_duplicate_intents() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Store::open(&d.path().join("source"), 1).unwrap();
    s.restore_business_state(support::state(), "test-fixture")
        .await
        .unwrap();
    s.bind_cluster("history-test").await.unwrap();
    let plan = plan();
    let timed = |time, command| Command::Authorized {
        authorization: converyn_core::identities::registration::Authorization {
            request_id: None,
            observed_ms: Some(time),
            token_sha256: None,
            principal: "peer".into(),
            fingerprint: "fixture".into(),
        },
        command: Box::new(command),
    };
    assert!(
        apply(
            &mut s,
            1,
            timed(
                100,
                Command::CreateRun {
                    output_key: None,
                    run_id: "old".into(),
                    idempotency_key: "once".into(),
                    plan: Box::new(plan.clone())
                }
            )
        )
        .await
        .ok
    );
    assert!(
        apply(
            &mut s,
            2,
            timed(
                200,
                Command::Cancel {
                    run_id: "old".into()
                }
            )
        )
        .await
        .ok
    );
    let before = s.machine().await.unwrap().state.runs["old"].clone();
    assert_eq!(before.submitted_ms, Some(100));
    assert_eq!(before.finished_ms, Some(200));
    assert!(
        apply(
            &mut s,
            3,
            Command::History {
                change: Change::Archive {
                    kind: Kind::Run,
                    uid: "old".into(),
                    expected_revision: before.revision,
                    now_ms: 1000
                }
            }
        )
        .await
        .ok
    );
    assert!(!s.machine().await.unwrap().state.runs.contains_key("old"));
    assert_eq!(s.load_run("old").await.unwrap().unwrap(), before);
    let repeated = apply(
        &mut s,
        4,
        Command::CreateRun {
            output_key: None,
            run_id: "duplicate".into(),
            idempotency_key: "once".into(),
            plan: Box::new(plan),
        },
    )
    .await;
    assert!(repeated.ok);
    assert_eq!(repeated.run_id.as_deref(), Some("old"));
    assert!(
        !s.machine()
            .await
            .unwrap()
            .state
            .runs
            .contains_key("duplicate")
    );
    let snapshot = s.build_snapshot().await.unwrap();
    let mut replica = Store::open(&d.path().join("replica"), 2).unwrap();
    replica
        .install_snapshot(&snapshot.meta, snapshot.snapshot)
        .await
        .unwrap();
    assert_eq!(replica.load_run("old").await.unwrap().unwrap(), before);
    let (file, sha) = s.backup_carrier().await.unwrap();
    assert_eq!(
        converyn_runtime::storage::backup::inspect(&file).unwrap(),
        (sha, "history-test".into())
    );
    let restored = d.path().join("restored");
    converyn_runtime::storage::backup::restore(&file, &restored, 3, true)
        .await
        .unwrap();
    let recovered = Store::open(&restored, 3).unwrap();
    assert_eq!(recovered.load_run("old").await.unwrap().unwrap(), before);
    assert!(
        apply(
            &mut s,
            5,
            timed(
                9999,
                Command::Cancel {
                    run_id: "old".into()
                }
            )
        )
        .await
        .ok
    );
    assert_eq!(s.machine().await.unwrap().state.runs["old"], before);
    let db = rusqlite::Connection::open(d.path().join("source/state.sqlite3")).unwrap();
    db.execute("UPDATE cold_history SET data=?1", [b"corrupt".as_slice()])
        .unwrap();
    drop(db);
    // A cold read on the other replica stays intact; corruption is never
    // presented as a missing Run, empty receipt or successful replacement.
    assert_eq!(replica.load_run("old").await.unwrap().unwrap(), before);
    let m = s.machine().await.unwrap();
    let record = m.state.history.get(Kind::Run, "old").unwrap().clone();
    assert!(s.history_value(record).await.is_err());
    assert!(s.build_snapshot().await.is_err());
}
#[tokio::test]
async fn definitions_remain_usable_after_archiving() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Store::open(d.path(), 1).unwrap();
    s.restore_business_state(support::state(), "test-fixture")
        .await
        .unwrap();
    let text = "retained definition\n";
    use sha2::{Digest, Sha256};
    let sha = hex::encode(Sha256::digest(text));
    assert!(
        apply(
            &mut s,
            1,
            Command::PutScript {
                sha256: sha.clone(),
                content: text.into()
            }
        )
        .await
        .ok
    );
    assert!(
        apply(
            &mut s,
            2,
            Command::History {
                change: Change::Archive {
                    kind: Kind::Definition,
                    uid: sha.clone(),
                    expected_revision: 0,
                    now_ms: 1
                }
            }
        )
        .await
        .ok
    );
    assert_eq!(s.definition(&sha).await.unwrap().as_deref(), Some(text));
    assert!(s.machine().await.unwrap().state.has_definition(&sha));
}

#[tokio::test]
async fn resident_view_keeps_an_index_instead_of_all_300_node_run_payloads() {
    let d = tempfile::tempdir().unwrap();
    let mut s = Store::open(d.path(), 1).unwrap();
    s.restore_business_state(support::state(), "test-fixture")
        .await
        .unwrap();
    let nodes = (0..300)
        .map(|i| serde_json::json!({"uid":format!("node-{i}"),"hostname":format!("node-{i}"),"address":format!("192.0.{}.{}",i/254,i%254+1)}))
        .collect::<Vec<_>>();
    let input:PlanInput=serde_json::from_value(serde_json::json!({"owner_uid":"app","hosts":{"node_group_uid":"business","revision":1,"node_uids":(0..300).map(|i| format!("node-{i}")).collect::<Vec<_>>()},"inventory":{"nodes":nodes,"groups":{"worker":(0..300).map(|i| format!("node-{i}")).collect::<Vec<_>>()}},"batch_size":10,"parameters":{},"playbook":{"api_version":"converyn/v1","name":"history-size","steps":[{"id":"config","phase":"rollout","scope":"per_node","operation":{"kind":"file","path":"/opt/app/config","sha256":"a".repeat(64),"mode":420},"hosts":"worker"}]}})).unwrap();
    let p = input.compile().unwrap();
    let mut position = 0;
    let mut payload_bytes = 0;
    for i in 0..64 {
        let uid = format!("run-{i}");
        position += 1;
        assert!(
            apply(
                &mut s,
                position,
                Command::CreateRun {
                    output_key: None,
                    run_id: uid.clone(),
                    idempotency_key: uid.clone(),
                    plan: Box::new(p.clone())
                }
            )
            .await
            .ok
        );
        position += 1;
        assert!(
            apply(
                &mut s,
                position,
                Command::Cancel {
                    run_id: uid.clone()
                }
            )
            .await
            .ok
        );
        let run = s.machine().await.unwrap().state.runs[&uid].clone();
        payload_bytes += serde_json::to_vec(&run).unwrap().len();
        position += 1;
        assert!(
            apply(
                &mut s,
                position,
                Command::History {
                    change: Change::Archive {
                        kind: Kind::Run,
                        uid,
                        expected_revision: run.revision,
                        now_ms: position
                    }
                }
            )
            .await
            .ok
        );
    }
    let m = s.machine().await.unwrap();
    assert!(m.state.runs.is_empty());
    assert_eq!(m.state.history.records.len(), 64);
    let resident = serde_json::to_vec(&m.state).unwrap().len();
    assert!(
        resident * 20 < payload_bytes,
        "resident={resident}, historical_payload={payload_bytes}"
    );
    assert_eq!(
        s.load_run("run-0").await.unwrap().unwrap().plan.work.len(),
        300
    );
}
