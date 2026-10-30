//! Managed nodes trust approved Work; only explicit host steps select root.
use converyn_core::model::domain::Operation;
use converyn_runtime::{
    executors::{authorization, native},
    nodes::agent::Config,
};
use serde_json::json;
fn config() -> Config {
    serde_json::from_value(json!({"node_uid":"node","cluster":"control","servers":["https://127.0.0.1:18443"],"state_dir":"/var/lib/converyn/state","token_file":"/etc/converyn/token","tls":{"cert":"/etc/converyn/cert","key":"/etc/converyn/key","ca":"/etc/converyn/ca"},"execution":{"uid":65534,"gid":65534,"mode":"systemd","work_root":"/var/lib/converyn/tasks"}})).unwrap()
}
fn operation(host: bool) -> Operation {
    serde_json::from_value(json!({"kind":"exec","execution":if host {"host"}else{"user"},"script_sha256":"a".repeat(64),"timeout_seconds":60,"environment":{"VERSION":"one"}})).unwrap()
}
#[test]
fn user_is_default_and_host_is_explicit_without_local_grants() {
    let c = config();
    let ordinary: Operation = serde_json::from_value(
        json!({"kind":"exec","script_sha256":"a".repeat(64),"timeout_seconds":60}),
    )
    .unwrap();
    assert_eq!(
        authorization::policy(&c, &ordinary, Some("default"))
            .unwrap()
            .unwrap()
            .uid,
        65534
    );
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let selected = authorization::policy(&c, &operation(true), Some("default"))
        .unwrap()
        .unwrap();
    assert_eq!(selected.uid, 0);
    assert!(selected.privileged);
    assert!(selected.writable_paths.is_empty());
    assert!(selected.task_path().split(':').any(|p| p == "/usr/sbin"));
}
#[test]
fn host_execution_requires_supervision_and_rejects_startup_injection() {
    let mut c = config();
    c.execution.as_mut().unwrap().mode = converyn_runtime::executors::execution::Mode::Process;
    assert!(authorization::policy(&c, &operation(true), Some("default")).is_err());
    for name in ["PATH", "BASH_ENV", "LD_PRELOAD", "PYTHONPATH"] {
        let mut work = operation(true);
        if let Operation::Exec { environment, .. } = &mut work {
            environment.insert(name.into(), "/tmp/unsafe".into());
        }
        assert!(authorization::policy(&config(), &work, Some("default")).is_err());
    }
}
#[test]
fn historical_profiles_are_readable_but_cannot_authorize_execution() {
    let op:Operation=serde_json::from_value(json!({"kind":"exec","execution_profile":"install","script_sha256":"a".repeat(64),"timeout_seconds":60})).unwrap();
    assert!(
        serde_json::to_value(&op)
            .unwrap()
            .get("execution")
            .is_none()
    );
    assert!(authorization::policy(&config(), &op, Some("default")).is_err());
}
#[test]
fn native_permissions_are_derived_from_exact_approved_operations() {
    let identity = converyn_runtime::plugins::registry()
        .get("converyn.linux-host")
        .unwrap()
        .descriptor()
        .identity;
    let call = converyn_runtime::plugins::registry()
        .get(&identity.id)
        .unwrap()
        .compile("directories", json!({"paths":["/opt/test-app"]}))
        .unwrap();
    let op = Operation::Plugin {
        call: Box::new(call),
    };
    let p = native::managed_policy(&op, Some("default")).unwrap();
    let grant = &p.plugins[&identity.id];
    assert_eq!(grant.actions.len(), 1);
    assert_eq!(grant.policy["directories"], json!(["/opt/test-app"]));
    let file: Operation = serde_json::from_value(
        json!({"kind":"file","path":"/opt/test/config","sha256":"a".repeat(64),"mode":420}),
    )
    .unwrap();
    assert_eq!(
        native::managed_policy(&file, Some("default"))
            .unwrap()
            .file_paths,
        vec![std::path::PathBuf::from("/opt/test/config")]
    );
}
#[test]
fn removed_local_authorization_fields_are_rejected() {
    let mut value = serde_json::to_value(config()).unwrap();
    for name in ["native_policy", "execution_profiles", "secrets"] {
        value[name] = json!({});
        assert!(serde_json::from_value::<Config>(value.clone()).is_err());
        value.as_object_mut().unwrap().remove(name);
    }
}

#[test]
fn systemd_privilege_boundary_allows_host_domain_transitions_only() {
    if unsafe { libc::geteuid() } != 0 {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    for host in [false, true] {
        let mut c = config();
        c.execution.as_mut().unwrap().work_root =
            root.path().join(if host { "host" } else { "user" });
        let policy = authorization::policy(&c, &operation(host), Some("default"))
            .unwrap()
            .unwrap();
        let journal = root
            .path()
            .join(if host { "host-journal" } else { "user-journal" });
        std::fs::create_dir(&journal).unwrap();
        let (command, _, _) = policy
            .command("privilege-check", b"true\n", 30, &journal)
            .unwrap();
        let args = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.contains(&format!(
            "--property=NoNewPrivileges={}",
            if host { "no" } else { "yes" }
        )));
    }
}
