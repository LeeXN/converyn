mod support;
use converyn_core::model::domain::{Operation, PlanInput};
use converyn_runtime::credentials::secrets::{Value, resolve_literals as resolve};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, sync::atomic::AtomicBool};
fn input() -> PlanInput {
    serde_json::from_value(serde_json::json!({"project":"team","owner_uid":"app","parameters":{"X":"release","COMMON":"release"},"batch_size":1,"playbook":{"api_version":"converyn/v1","name":"variables","steps":[{"id":"script","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":"a".repeat(64),"timeout_seconds":10,"environment":{"X":"step","PASSWORD":{"secret":"database","version":"v1"}}},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}],"groups":{"worker":["node"]}},"hosts":{"node_group_uid":"cluster","revision":1,"node_uids":["node"]}})).unwrap()
}
#[test]
fn final_environment_is_frozen_and_unsupported_plan_formats_are_rejected() {
    let plan = input().compile().unwrap();
    plan.verify().unwrap();
    let Operation::Exec { environment, .. } = &plan.work[0].operation else {
        panic!()
    };
    assert_eq!(environment["X"], Value::from("step"));
    assert!(!environment.contains_key("NODE_ONLY"));
    assert_eq!(environment["COMMON"], Value::from("release"));
    assert!(matches!(environment["PASSWORD"], Value::Secret(_)));
    let mut unsupported = plan.clone();
    unsupported.format = 1;
    assert!(unsupported.verify().is_err());
}

#[test]
fn workflow_environment_is_visible_in_real_process_logs_without_evaluating_values() {
    use converyn_runtime::executors::runner::{Options, Outcome, run_script_options};
    let d = tempfile::tempdir().unwrap();
    let marker = d.path().join("must-not-exist");
    let value = format!("version'$(touch {})", marker.display());
    let variables = converyn_core::orchestration::values::environment(&BTreeMap::from([(
        "VERSION".into(),
        serde_json::json!(value),
    )]))
    .unwrap();
    let environment = resolve(&variables).unwrap();
    let script = d.path().join("script.sh");
    let content = "printf 'actual=%s\\n' \"$VERSION\"\n";
    std::fs::write(&script, content).unwrap();
    let receipt = run_script_options(
        &d.path().join("state"),
        "workflow-values",
        &script,
        &hex::encode(Sha256::digest(content)),
        10,
        &AtomicBool::new(false),
        Options {
            outputs: None,
            policy: None,
            environment: Some(&environment),
        },
    )
    .unwrap();
    assert_eq!(receipt.outcome, Outcome::Succeeded);
    let log = std::fs::read_to_string(
        d.path()
            .join("state")
            .join(hex::encode(Sha256::digest("workflow-values")))
            .join("stdout.log"),
    )
    .unwrap();
    assert!(log.contains("# Converyn effective variables"));
    assert!(log.contains(&format!("actual={value}")));
    assert!(!marker.exists());
    let rendered = d.path().join("rendered.sh");
    std::fs::write(
        &rendered,
        converyn_core::orchestration::values::render(content, &variables),
    )
    .unwrap();
    let output = std::process::Command::new("/bin/bash")
        .arg(&rendered)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("actual={value}\n")
    );
    assert!(!marker.exists());
    let secret = "quoted\"secret\nvalue";
    let environment = converyn_runtime::credentials::secrets::Resolved {
        values: BTreeMap::from([
            ("WORKFLOW_VALUES_JSON".into(), "{}".into()),
            ("TOKEN".into(), secret.into()),
        ]),
        redactions: vec![secret.as_bytes().to_vec()],
    };
    let prefix = String::from_utf8(converyn_runtime::executors::task_log::environment_prefix(
        &environment,
    ))
    .unwrap();
    assert!(!prefix.contains("quoted"));
    assert!(prefix.contains("[REDACTED]"));
}
#[test]
fn centralized_secret_input_is_redacted_in_real_process_output() {
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o711)).unwrap();
    let secret = "fixture-secret-do-not-log-7a19";
    let variables = BTreeMap::from([(
        "PASSWORD".into(),
        Value::Secret(converyn_runtime::credentials::secrets::Reference {
            secret: "database".into(),
            version: "v1".into(),
        }),
    )]);
    assert!(resolve(&variables).is_err());
    let key = d.path().join("keys.json");
    let public = converyn_runtime::credentials::outputs::Keyring::generate(&key, false).unwrap();
    let sealed = converyn_runtime::credentials::outputs::seal(
        &public,
        &serde_json::json!(secret),
        &converyn_runtime::credentials::secrets::binding("database", "v1"),
    )
    .unwrap();
    let keys = converyn_runtime::credentials::outputs::Keyring::load(&key).unwrap();
    let plain =
        converyn_runtime::credentials::secrets::open(&keys, "database", "v1", &sealed).unwrap();
    let environment = converyn_runtime::credentials::secrets::Resolved {
        values: [("PASSWORD".into(), plain)].into(),
        redactions: vec![secret.as_bytes().to_vec()],
    };
    let script = d.path().join("script");
    let content = b"set -eu\nprintf 'password=%s\\n' \"$PASSWORD\"\n";
    std::fs::write(&script, content).unwrap();
    let execution = converyn_runtime::executors::execution::Policy {
        privileged: false,
        uid: 65534,
        gid: 65534,
        mode: converyn_runtime::executors::execution::Mode::Process,
        work_root: d.path().join("tasks"),
        home: None,
        writable_paths: vec![],
        memory_bytes: 128 * 1024 * 1024,
        tasks_max: 64,
    };
    let state = d.path().join("state");
    let receipt = converyn_runtime::executors::runner::run_script_options(
        &state,
        "secret-task",
        &script,
        &hex::encode(Sha256::digest(content)),
        10,
        &AtomicBool::new(false),
        converyn_runtime::executors::runner::Options {
            outputs: None,
            policy: Some(&execution),
            environment: Some(&environment),
        },
    )
    .unwrap();
    assert_eq!(
        receipt.outcome,
        converyn_runtime::executors::runner::Outcome::Succeeded
    );
    let serialized = serde_json::to_string(&receipt).unwrap();
    assert!(!serialized.contains(secret));
    assert!(serialized.contains("[REDACTED]"));
    let log = state
        .join(hex::encode(Sha256::digest("secret-task")))
        .join("stdout.log");
    let bytes = std::fs::read_to_string(log).unwrap();
    assert_eq!(bytes, "password=[REDACTED]\n");
    assert!(!serde_json::to_string(&variables).unwrap().contains(secret));
}
