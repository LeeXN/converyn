mod support;
use converyn_core::orchestration::{
    administration::Change,
    machine::{Command, State},
};
use converyn_runtime::{
    credentials::{
        outputs::{Keyring, seal},
        secrets,
    },
    storage::store::Store,
};
use serde_json::json;
#[test]
fn encrypted_versions_are_immutable_and_bound_to_their_names() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("keys.json");
    let public = Keyring::generate(&key, false).unwrap();
    let keys = Keyring::load(&key).unwrap();
    let value = "private-password-fixture";
    let sealed = seal(&public, &json!(value), &secrets::binding("database", "v1")).unwrap();
    assert!(!serde_json::to_string(&sealed).unwrap().contains(value));
    assert_eq!(
        secrets::open(&keys, "database", "v1", &sealed).unwrap(),
        value
    );
    assert!(secrets::open(&keys, "other", "v1", &sealed).is_err());
    assert!(secrets::open(&keys, "database", "v2", &sealed).is_err());
    let mut state = crate::support::state();
    let change = Change::Secret {
        name: "database".into(),
        version: "v1".into(),
        sealed: sealed.clone(),
    };
    state.administer(change.clone()).unwrap();
    state.administer(change).unwrap();
    let different = seal(
        &public,
        &json!("other-value"),
        &secrets::binding("database", "v1"),
    )
    .unwrap();
    assert!(
        state
            .administer(Change::Secret {
                name: "database".into(),
                version: "v1".into(),
                sealed: different
            })
            .is_err()
    );
    let json = serde_json::to_string(&state).unwrap();
    assert!(!json.contains(value));
    let restored: State = serde_json::from_str(&json).unwrap();
    assert_eq!(
        restored.administration.secrets,
        state.administration.secrets
    );
}
#[tokio::test]
async fn encrypted_secret_survives_sqlite_reopen_and_backup() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("keys.json");
    let public = Keyring::generate(&key, false).unwrap();
    let sealed = seal(
        &public,
        &json!("sqlite-private-secret"),
        &secrets::binding("database", "v1"),
    )
    .unwrap();
    let directory = root.path().join("store");
    let store = Store::open(&directory, 1).unwrap();
    let mut state = crate::support::state();
    state.apply(Command::Administer {
        change: Change::Secret {
            name: "database".into(),
            version: "v1".into(),
            sealed,
        },
    });
    store
        .restore_business_state(state.clone(), &"a".repeat(64))
        .await
        .unwrap();
    drop(store);
    let reopened = Store::open(&directory, 1).unwrap();
    let machine = reopened.machine().await.unwrap();
    assert_eq!(
        machine.state.administration.secrets,
        state.administration.secrets
    );
    let bytes = std::fs::read(directory.join("state.sqlite3")).unwrap();
    assert!(
        !bytes
            .windows(b"sqlite-private-secret".len())
            .any(|v| v == b"sqlite-private-secret")
    );
}
#[tokio::test]
async fn restore_requires_original_keys_before_creating_a_store() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("keys.json");
    let public = Keyring::generate(&key, false).unwrap();
    let sealed = seal(
        &public,
        &json!("restore-private-secret"),
        &secrets::binding("database", "v1"),
    )
    .unwrap();
    let directory = root.path().join("store");
    let store = Store::open(&directory, 1).unwrap();
    store.bind_cluster("test").await.unwrap();
    let mut state = crate::support::state();
    state
        .administer(Change::Secret {
            name: "database".into(),
            version: "v1".into(),
            sealed,
        })
        .unwrap();
    store
        .restore_business_state(state, &"a".repeat(64))
        .await
        .unwrap();
    let (backup, _) = store.backup_carrier().await.unwrap();
    let missing = root.path().join("missing-key-restore");
    assert!(
        converyn_runtime::storage::backup::restore(&backup, &missing, 2, true)
            .await
            .is_err()
    );
    assert!(!missing.exists());
    let restored = root.path().join("restored");
    converyn_runtime::storage::backup::restore_with_keyring(
        &backup,
        &restored,
        2,
        true,
        Some(&key),
    )
    .await
    .unwrap();
    let restored = Store::open(&restored, 2).unwrap();
    assert!(
        restored
            .machine()
            .await
            .unwrap()
            .state
            .administration
            .secrets
            .contains_key("database")
    );
}
#[tokio::test]
async fn old_schema_is_rejected_without_mutating_its_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("store");
    let store = Store::open(&directory, 1).unwrap();
    drop(store);
    let db = rusqlite::Connection::open(directory.join("state.sqlite3")).unwrap();
    db.execute(
        "UPDATE metadata SET value=?1 WHERE key='schema_version'",
        [serde_json::to_vec(&21).unwrap()],
    )
    .unwrap();
    assert!(converyn_runtime::storage::access_migration::migrate(&directory).is_err());
    assert!(Store::open(&directory, 1).is_err());
}
