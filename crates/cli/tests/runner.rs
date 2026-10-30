use converyn_runtime::executors::runner::{Outcome, run_script};
use sha2::{Digest, Sha256};
use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::tempdir;

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[test]
fn receipt_prevents_replay_and_conflicting_intent() {
    let d = tempdir().unwrap();
    let marker = d.path().join("effects");
    let script = d.path().join("script.sh");
    let body = format!("printf 'effect\\n' >> '{}'\n", marker.display());
    fs::write(&script, &body).unwrap();
    let state = d.path().join("state");
    for _ in 0..2 {
        assert_eq!(
            run_script(&state, "run/work", &script, &digest(body.as_bytes()), 10)
                .unwrap()
                .outcome,
            Outcome::Succeeded
        );
    }
    assert_eq!(fs::read_to_string(&marker).unwrap(), "effect\n");
    fs::write(&script, "exit 0\n").unwrap();
    assert!(run_script(&state, "run/work", &script, &digest(b"exit 0\n"), 10).is_err());
}

#[test]
fn bad_hash_and_failed_script_do_not_retry_effects() {
    let d = tempdir().unwrap();
    let script = d.path().join("script.sh");
    let marker = d.path().join("effects");
    let body = format!("echo effect >> '{}'\nexit 42\n", marker.display());
    fs::write(&script, &body).unwrap();
    let state = d.path().join("state");
    assert!(run_script(&state, "op", &script, &"0".repeat(64), 10).is_err());
    assert!(!marker.exists());
    for _ in 0..2 {
        let r = run_script(&state, "op", &script, &digest(body.as_bytes()), 10).unwrap();
        assert_eq!(r.outcome, Outcome::Failed);
        assert_eq!(r.exit_code, Some(42));
    }
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
}

#[test]
fn timeout_stops_process_group() {
    let d = tempdir().unwrap();
    let script = d.path().join("script.sh");
    let marker = d.path().join("unexpected");
    let body = format!("(sleep 2; touch '{}') &\nwait\n", marker.display());
    fs::write(&script, &body).unwrap();
    let r = run_script(
        &d.path().join("state"),
        "op",
        &script,
        &digest(body.as_bytes()),
        1,
    )
    .unwrap();
    assert_eq!(r.outcome, Outcome::TimedOut);
    thread::sleep(Duration::from_secs(2));
    assert!(!marker.exists());
}

#[test]
fn killed_runner_retains_uncertainty_without_reexecution() {
    let d = tempdir().unwrap();
    let script = d.path().join("script.sh");
    let marker = d.path().join("effects");
    let pid = d.path().join("child.pid");
    let body = format!(
        "echo $$ > '{}'\necho effect >> '{}'\nsleep 60\n",
        pid.display(),
        marker.display()
    );
    fs::write(&script, &body).unwrap();
    let hash = digest(body.as_bytes());
    let state = d.path().join("state");
    let mut runner = Command::new(env!("CARGO_BIN_EXE_converyn"))
        .args([
            "admin",
            "run-script",
            "--operation-id",
            "op",
            "--sha256",
            &hash,
        ])
        .arg("--script")
        .arg(&script)
        .arg("--state-dir")
        .arg(&state)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(marker.exists(), "runner did not start the script");
    assert!(run_script(&state, "op", &script, &hash, 60).is_err());
    runner.kill().unwrap();
    runner.wait().unwrap();
    assert!(marker.exists());
    let child: i32 = fs::read_to_string(pid).unwrap().trim().parse().unwrap();
    // The harness explicitly cleans up the still-running group. A receipt alone
    // never claims the external side effect has stopped after losing the runner.
    unsafe {
        libc::kill(-child, libc::SIGKILL);
    }
    fs::remove_file(&script).unwrap();
    let r = run_script(&state, "op", &script, &hash, 60).unwrap();
    assert_eq!(r.outcome, Outcome::NeedsAttention);
    assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
}

#[test]
fn terminal_receipt_survives_missing_upload() {
    let d = tempdir().unwrap();
    let script = d.path().join("source.sh");
    fs::write(&script, "exit 0\n").unwrap();
    let state = d.path().join("state");
    let hash = digest(b"exit 0\n");
    assert_eq!(
        run_script(&state, "op", &script, &hash, 5).unwrap().outcome,
        Outcome::Succeeded
    );
    fs::remove_file(&script).unwrap();
    assert_eq!(
        run_script(&state, "op", &script, &hash, 5).unwrap().outcome,
        Outcome::Succeeded
    );
    assert!(run_script(&state, "op", &script, &hash, 6).is_err());
    assert!(run_script(&state, "op", &script, &"0".repeat(64), 5).is_err());
}

#[test]
fn bounded_logs_keep_disk_prefix_and_remote_tail() {
    use sha2::{Digest, Sha256};
    let d = tempfile::tempdir().unwrap();
    let script = d.path().join("script.sh");
    let bytes=b"head -c 10000000 /dev/zero | tr '\\0' x; printf '\\nfinal-marker\\n'; printf 'stderr-marker\\n' >&2\n";
    std::fs::write(&script, bytes).unwrap();
    let r = converyn_runtime::executors::runner::run_script(
        d.path().join("state").as_path(),
        "logs",
        &script,
        &hex::encode(Sha256::digest(bytes)),
        20,
    )
    .unwrap();
    assert_eq!(
        r.outcome,
        converyn_runtime::executors::runner::Outcome::Succeeded
    );
    let logs = r.logs.unwrap();
    assert!(logs.valid());
    assert_eq!(
        logs.stdout.stored_bytes,
        converyn_runtime::executors::task_log::LOG_LIMIT
    );
    assert!(logs.stdout.truncated);
    assert!(logs.stdout.tail.ends_with("final-marker\n"));
    assert_eq!(logs.stderr.tail, "stderr-marker\n");
}
#[test]
fn root_supervisor_drops_identity_groups_and_cannot_expose_private_state() {
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o711)).unwrap();
    let secret = d.path().join("controller-token");
    std::fs::write(&secret, b"must-not-leak").unwrap();
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
    let script = d.path().join("script.sh");
    let content = format!(
        "set -eu\ntest $(id -u) = 65534\ntest $(id -g) = 65534\n! cat '{}' 2>/dev/null\ngrep '^NoNewPrivs:' /proc/self/status\nprintf 'isolated-ok\\n'\n",
        secret.display()
    );
    std::fs::write(&script, &content).unwrap();
    let policy = converyn_runtime::executors::execution::Policy {
        privileged: false,
        uid: 65534,
        gid: 65534,
        mode: converyn_runtime::executors::execution::Mode::Process,
        work_root: d.path().join("tasks"),
        home: None,
        writable_paths: vec![],
        memory_bytes: 128 * 1024 * 1024,
        tasks_max: 128,
    };
    let receipt = converyn_runtime::executors::runner::run_script_with_policy(
        &d.path().join("state"),
        "isolated",
        &script,
        &hex::encode(Sha256::digest(content.as_bytes())),
        10,
        &std::sync::atomic::AtomicBool::new(false),
        Some(&policy),
    )
    .unwrap();
    assert_eq!(
        receipt.outcome,
        converyn_runtime::executors::runner::Outcome::Succeeded
    );
    assert_eq!(receipt.execution.unwrap().uid, 65534);
    let output = receipt.logs.unwrap().stdout.tail;
    assert!(
        output.contains("NoNewPrivs:\t1")
            && output.contains("isolated-ok")
            && !output.contains("must-not-leak")
    );
}
