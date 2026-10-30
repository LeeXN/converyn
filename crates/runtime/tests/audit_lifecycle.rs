mod support;
use converyn_core::orchestration::machine::Command;
use converyn_runtime::storage::{audit_store, store::Store};
use openraft::{CommittedLeaderId, Entry, EntryPayload, LogId, storage::RaftStateMachine};
use serde_json::json;
#[tokio::test]
async fn old_audit_limits_do_not_block_and_cold_proof_survives_carrier_restore() {
    let d = tempfile::tempdir().unwrap();
    let mut state = crate::support::state();
    state.registry.audit = (0..20001)
        .map(|i| json!({"action":"old","index":i,"principal":"unknown"}))
        .collect();
    state.administration.audit = vec![
        serde_json::from_value(
            json!({"action":"maintenance","node_uid":"node","enabled":false})
        )
        .unwrap();
        10001
    ];
    let mut store = Store::open(&d.path().join("source"), 1).unwrap();
    store.bind_cluster("audit-test").await.unwrap();
    store.restore_business_state(state, "a").await.unwrap();
    let reply = store
        .apply([Entry {
            log_id: LogId::new(CommittedLeaderId::new(1, 1), 1),
            payload: EntryPayload::Normal(Command::PutScript {
                sha256: converyn_core::model::domain::digest(&"x"),
                content: "x".into(),
            }),
        }])
        .await
        .unwrap();
    assert!(!reply[0].ok); // Even rejected committed requests have an immutable audit record.
    let m = store.machine().await.unwrap();
    assert_eq!(m.state.registry.audit.len(), audit_store::HOT);
    assert_eq!(m.state.administration.audit.len(), audit_store::HOT);
    assert_eq!(
        m.state.cold_audit["identity"].count,
        20001 - audit_store::HOT as u64
    );
    let rows = store.audit_page("identity".into(), 0, 100).await.unwrap();
    assert_eq!(rows.len(), 100);
    assert_eq!(rows[0]["index"], 0);
    let (path, _) = store.backup_carrier().await.unwrap();
    converyn_runtime::storage::carrier::validate(&path).unwrap();
    let target = d.path().join("restore");
    converyn_runtime::storage::backup::restore(&path, &target, 2, true)
        .await
        .unwrap();
    let restored = Store::open(&target, 2).unwrap();
    assert_eq!(
        restored
            .audit_page("identity".into(), 0, 100)
            .await
            .unwrap(),
        rows
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "DELETE FROM cold_history WHERE kind='audit' AND uid='identity/0000000000000000'",
        [],
    )
    .unwrap();
    drop(db);
    assert!(converyn_runtime::storage::carrier::validate(&path).is_err());
}
#[tokio::test]
async fn late_log_finalization_rehydrates_the_original_cold_run() {
    use converyn_core::{
        model::remote_logs::Manifest,
        orchestration::{
            administration::Change as Admin,
            history::{Change, Kind},
        },
    };
    use sha2::{Digest, Sha256};
    let d = tempfile::tempdir().unwrap();
    let mut s = crate::support::state();
    let input:converyn_core::model::domain::PlanInput=serde_json::from_value(json!({"project":"team","owner_uid":"app","batch_size":1,"playbook":{"api_version":"converyn/v1","name":"fixture","steps":[{"id":"health","phase":"rollout","scope":"per_node","operation":{"kind":"http_check","url":"https://example.invalid/","expected_status":200,"timeout_seconds":1},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}],"groups":{"worker":["node"]}},"hosts":{"node_group_uid":"hosts","revision":1,"node_uids":["node"]}})).unwrap();
    assert!(
        s.apply(Command::CreateRun {
            output_key: None,
            run_id: "old".into(),
            idempotency_key: "old".into(),
            plan: Box::new(input.compile().unwrap())
        })
        .ok
    );
    let run = std::sync::Arc::make_mut(s.runs.get_mut("old").unwrap());
    run.status = converyn_core::orchestration::machine::Status::Succeeded;
    let work = run.plan.work[0].id.clone();
    let a = std::sync::Arc::make_mut(run.attempts.get_mut(&work).unwrap());
    a.status = converyn_core::orchestration::machine::Status::Succeeded;
    a.grant = Some("grant".into());
    let sha = hex::encode(Sha256::digest(b""));
    a.receipt=Some(serde_json::from_value(json!({"operation_id":format!("old/{work}"),"script_sha256":"a".repeat(64),"timeout_seconds":1,"outcome":"succeeded","exit_code":0,"boot_id":"boot","logs":{"stdout":{"stored_bytes":0,"seen_bytes":0,"sha256":sha,"tail":"","truncated":false},"stderr":{"stored_bytes":0,"seen_bytes":0,"sha256":sha,"tail":"","truncated":false}}})).unwrap());
    s.execution_locks.clear();
    s.log_store_id = Some("a".repeat(64));
    let mut store = Store::open(d.path(), 1).unwrap();
    store.restore_business_state(s, "test").await.unwrap();
    let apply = |index, command| Entry {
        log_id: LogId::new(CommittedLeaderId::new(1, 1), index),
        payload: EntryPayload::Normal(command),
    };
    assert!(
        store
            .apply([apply(
                1,
                Command::History {
                    change: Change::Archive {
                        kind: Kind::Run,
                        uid: "old".into(),
                        expected_revision: 1,
                        now_ms: 1
                    }
                }
            )])
            .await
            .unwrap()[0]
            .ok
    );
    assert!(
        !store
            .machine()
            .await
            .unwrap()
            .state
            .runs
            .contains_key("old")
    );
    let manifest = Manifest {
        chunk_hashes: vec![],
        missing_ranges: vec![],
        run_id: "old".into(),
        work_id: work,
        node_uid: "node".into(),
        grant: "grant".into(),
        store_id: "a".repeat(64),
        stream: "stdout".into(),
        bytes: 0,
        sha256: sha,
        chunks: 0,
        truncated: false,
        expires_ms: 123,
    };
    let original = store.load_run("old").await.unwrap().unwrap();
    assert!(
        store
            .apply([apply(
                2,
                Command::Administer {
                    change: Admin::LogFinal {
                        manifest: manifest.clone()
                    }
                }
            )])
            .await
            .unwrap()[0]
            .ok
    );
    assert_eq!(store.load_run("old").await.unwrap().unwrap(), original);
    assert_eq!(
        store.machine().await.unwrap().state.log_manifests[&manifest.key()],
        manifest
    );
}
