use converyn_core::orchestration::machine::Command;
use converyn_runtime::{
    Raft, SubmissionError, TypeConfig, controller::network::Network, storage::store::Store,
};
use openraft::{BasicNode, Config};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

async fn wait_for(raft: &Raft, predicate: impl Fn(Value) -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !predicate(raft.write_diagnostics()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
fn definition() -> Command {
    use sha2::{Digest, Sha256};
    Command::PutScript {
        sha256: hex::encode(Sha256::digest(b"hello")),
        content: "hello".into(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_sqlite_write_keeps_capacity_after_timeout_and_original_intent_can_be_retried() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open(d.path(), 1).unwrap();
    let network = Network {
        https: false,
        token: "a".repeat(32),
        cluster: "submission-test".into(),
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        store: Some(store.clone()),
        peer_fingerprints: Default::default(),
    };
    let inner = openraft::Raft::<TypeConfig>::new(
        1,
        Arc::new(Config::default().validate().unwrap()),
        network,
        store.clone(),
        store.clone(),
    )
    .await
    .unwrap();
    let raft = Raft::with_store(inner, store.clone());
    raft.initialize(BTreeMap::from([(
        1,
        BasicNode {
            addr: "127.0.0.1:1".into(),
        },
    )]))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while raft.ensure_linearizable().await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // An unrelated transaction stalls physical durability while the Raft
    // submitter continues receiving requests. No mock semaphore or fake Raft.
    let blocker = rusqlite::Connection::open(d.path().join("state.sqlite3")).unwrap();
    blocker.busy_timeout(Duration::from_secs(5)).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut writes = Vec::new();
    for _ in 0..24 {
        let raft = raft.clone();
        writes.push(tokio::spawn(async move {
            raft.client_write(definition()).await
        }));
    }
    wait_for(&raft, |v| v["ordinary_in_flight"] == 24).await;
    assert!(matches!(
        raft.client_write(definition()).await,
        Err(SubmissionError::Full)
    ));
    for index in 0..8 {
        let raft = raft.clone();
        writes.push(tokio::spawn(async move {
            raft.client_write(Command::ObserveNode {
                diagnostic: None,
                inventory: None,
                resources: None,
                inventory_checked_ms: None,
                capabilities: vec![],
                proof: None,
                hardware_sha256: None,
                node_uid: format!("node-{index}"),
                boot_id: "boot".into(),
                session_id: "session".into(),
                observed_ms: 1,
            })
            .await
        }));
    }
    wait_for(&raft, |v| v["in_flight"] == 32).await;
    assert!(matches!(
        raft.client_write(Command::Cancel {
            run_id: "none".into()
        })
        .await,
        Err(SubmissionError::Full)
    ));
    for write in writes {
        assert!(matches!(
            write.await.unwrap(),
            Err(SubmissionError::Pending)
        ));
    }
    assert_eq!(raft.write_diagnostics()["in_flight"], 32);
    blocker.execute_batch("COMMIT").unwrap();
    wait_for(&raft, |v| v["in_flight"] == 0).await;
    let diagnostics = raft.write_diagnostics();
    assert_eq!(diagnostics["outcome_pending"], 32);
    assert_eq!(diagnostics["committed"], 32);
    assert_eq!(diagnostics["failures"], 0);
    assert_eq!(store.machine().await.unwrap().state.scripts.len(), 1);
    assert_eq!(store.machine().await.unwrap().state.observations.len(), 8);
    assert!(raft.client_write(definition()).await.unwrap().data.ok);
    assert_eq!(store.machine().await.unwrap().state.scripts.len(), 1);
    raft.shutdown().await.unwrap();
}
