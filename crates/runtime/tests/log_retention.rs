mod support;
use converyn_runtime::executors::log_retention::collect;
use converyn_runtime::executors::runner::{Outcome, Receipt};
use std::{fs, os::unix::fs::symlink};
fn operation(root: &std::path::Path, id: char, outcome: Outcome) -> std::path::PathBuf {
    let d = root.join(id.to_string().repeat(64));
    fs::create_dir(&d).unwrap();
    fs::write(d.join("lock"), []).unwrap();
    let r = Receipt {
        outputs: Default::default(),
        operation_id: "op".into(),
        script_sha256: "a".repeat(64),
        timeout_seconds: 10,
        outcome,
        exit_code: Some(0),
        boot_id: "boot".into(),
        native: None,
        execution: None,
        logs: None,
        diagnostic: None,
    };
    fs::write(d.join("receipt.json"), serde_json::to_vec(&r).unwrap()).unwrap();
    fs::File::create(d.join("stdout.log"))
        .unwrap()
        .set_len(40 * 1024 * 1024)
        .unwrap();
    d
}
#[test]
fn raw_logs_can_be_pruned_but_execution_identity_is_never_deleted() {
    let d = tempfile::tempdir().unwrap();
    let done = operation(d.path(), 'a', Outcome::Succeeded);
    let receipt = fs::read(done.join("receipt.json")).unwrap();
    assert_eq!(collect(d.path(), 32 * 1024 * 1024).unwrap(), 0);
    assert!(!done.join("stdout.log").exists());
    assert_eq!(fs::read(done.join("receipt.json")).unwrap(), receipt);
    assert!(done.join("lock").exists());
}
#[test]
fn unknown_logs_and_symlink_targets_are_preserved() {
    let d = tempfile::tempdir().unwrap();
    let unknown = operation(d.path(), 'a', Outcome::NeedsAttention);
    assert!(collect(d.path(), 32 * 1024 * 1024).is_err());
    assert!(unknown.join("stdout.log").exists());
    let outside = d.path().join("outside");
    fs::write(&outside, b"keep").unwrap();
    symlink(&outside, unknown.join("stderr.log")).unwrap();
    assert!(collect(d.path(), 32 * 1024 * 1024).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"keep");
}

#[test]
fn native_and_shell_logs_share_one_quota_and_resolution_keeps_unknown_receipt() {
    use sha2::{Digest, Sha256};
    let d = tempfile::tempdir().unwrap();
    let native = d.path().join("native");
    fs::create_dir(&native).unwrap();
    let id = "run/native";
    let key = hex::encode(Sha256::digest(id));
    let r = Receipt {
        outputs: Default::default(),
        operation_id: id.into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        outcome: Outcome::NeedsAttention,
        exit_code: None,
        boot_id: "boot".into(),
        native: None,
        execution: None,
        logs: None,
        diagnostic: None,
    };
    let raw = serde_json::to_vec(&r).unwrap();
    fs::write(native.join(format!("{key}.json")), &raw).unwrap();
    fs::write(native.join(format!("{key}.lock")), []).unwrap();
    let log = native.join(format!("{key}.stdout.log"));
    fs::File::create(&log)
        .unwrap()
        .set_len(40 * 1024 * 1024)
        .unwrap();
    assert!(
        converyn_runtime::executors::log_retention::collect_agent(d.path(), 32 * 1024 * 1024)
            .is_err()
    );
    converyn_runtime::executors::log_retention::mark_resolved(d.path(), id, true, &r).unwrap();
    assert_eq!(
        converyn_runtime::executors::log_retention::collect_agent(d.path(), 32 * 1024 * 1024)
            .unwrap(),
        0
    );
    assert!(!log.exists());
    assert_eq!(fs::read(native.join(format!("{key}.json"))).unwrap(), raw);
}
