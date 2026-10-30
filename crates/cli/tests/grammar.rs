//! Exercise local/remote command boundaries through the actual executable.
use std::process::{Command, Output};

fn run(args: &[&str], home: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_converyn"))
        .args(args)
        .env("HOME", home)
        .env("CONVERYN_CONFIG", home.join("missing-config.json"))
        .env_remove("CONVERYN_CONTEXT")
        .current_dir(home)
        .output()
        .unwrap()
}

#[test]
fn local_tools_ignore_environment_config_and_reject_explicit_client_selection() {
    let root = tempfile::tempdir().unwrap();
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = repository.join("scripts/tests/fixtures/plan.yaml");
    let workflow = repository.join("example/workflow/workflow.yaml");
    for args in [
        vec!["get", "plugins", "--local", "-o", "json"],
        vec![
            "compile",
            "plans",
            "-f",
            input.to_str().unwrap(),
            "-o",
            "json",
        ],
        vec![
            "validate",
            "workflows",
            "-f",
            workflow.to_str().unwrap(),
            "-o",
            "json",
        ],
    ] {
        let result = run(&args, root.path());
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap();
        for selector in [["--config", "missing.json"], ["--context", "missing"]] {
            let mut explicit = args.clone();
            explicit.extend(selector);
            let rejected = run(&explicit, root.path());
            assert!(!rejected.status.success());
            assert!(
                String::from_utf8_lossy(&rejected.stderr).contains("not accepted by local tools")
            );
        }
    }
    // Default discovery must fail when the remote configuration is missing,
    // rather than silently returning the local registry.
    let remote = run(&["get", "plugins", "-o", "json"], root.path());
    assert!(!remote.status.success());
    assert!(remote.stdout.is_empty());
    assert!(!root.path().join("missing-config.json").exists());
}

#[cfg(feature = "full")]
#[test]
fn local_plugin_details_use_the_same_envelope_as_remote_discovery() {
    let root = tempfile::tempdir().unwrap();
    let list = run(&["get", "plugins", "--local", "-o", "json"], root.path());
    assert!(list.status.success());
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let id = list["plugins"][0]["identity"]["id"].as_str().unwrap();
    let detail = run(
        &["get", "plugins", id, "--local", "-o", "json"],
        root.path(),
    );
    assert!(detail.status.success());
    let detail: serde_json::Value = serde_json::from_slice(&detail.stdout).unwrap();
    assert_eq!(detail["plugins"].as_array().unwrap().len(), 1);
    assert_eq!(detail["plugins"][0], list["plugins"][0]);
    assert_eq!(detail["manifest_sha256"], list["manifest_sha256"]);
    assert!(
        !run(&["get", "plugins", "missing", "--local"], root.path())
            .status
            .success()
    );
}
