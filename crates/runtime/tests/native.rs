use converyn_core::model::domain::Operation;
use converyn_runtime::executors::native::{Policy, execute, reconcile_file};
use converyn_runtime::executors::runner::Outcome;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    sync::atomic::AtomicBool,
};
fn operation(path: &std::path::Path, bytes: &[u8], mode: u32) -> Operation {
    Operation::File {
        path: path.to_str().unwrap().into(),
        sha256: hex::encode(Sha256::digest(bytes)),
        mode,
    }
}
#[test]
fn exact_file_capability_does_not_authorize_neighbours_or_symlinks() {
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("nested/config");
    let policy = Policy {
        file_paths: vec![target.clone()],
        ..Policy::default()
    };
    let cancel = AtomicBool::new(false);
    assert!(
        reconcile_file(
            &policy,
            &operation(&target, b"config", 0o600),
            b"config",
            &cancel
        )
        .unwrap()
        .verified()
    );
    assert!(
        reconcile_file(
            &policy,
            &operation(&target.with_file_name("other"), b"config", 0o600),
            b"config",
            &cancel
        )
        .is_err()
    );
    fs::remove_file(&target).unwrap();
    let outside = d.path().join("outside");
    fs::write(&outside, b"preserve").unwrap();
    symlink(&outside, &target).unwrap();
    assert!(
        reconcile_file(
            &policy,
            &operation(&target, b"config", 0o600),
            b"config",
            &cancel
        )
        .is_err()
    );
    assert_eq!(fs::read(outside).unwrap(), b"preserve");
}
#[test]
fn installer_and_agent_reject_writes_to_their_configuration_unit_and_platform_identity() {
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("agent.json");
    let hardware = d.path().join("platform-id");
    fs::write(&hardware, b"operator-platform-identity").unwrap();
    let cfg:converyn_runtime::nodes::agent::Config=serde_json::from_value(serde_json::json!({"node_uid":"node","cluster":"control","servers":["https://127.0.0.1:18443"],"state_dir":d.path().join("state"),"token_file":d.path().join("token"),"tls":{"cert":d.path().join("cert"),"key":d.path().join("key"),"ca":d.path().join("ca")},"hardware_id_file":hardware})).unwrap();
    for target in [
        &config,
        std::path::Path::new("/etc/systemd/system/converyn-agent.service"),
        &hardware,
    ] {
        assert!(
            converyn_runtime::nodes::agent::validate_native_operation(
                &cfg,
                &config,
                &d.path().join("converyn"),
                &operation(target, b"config", 0o600),
                Some("default")
            )
            .is_err()
        );
    }
    converyn_runtime::nodes::agent::validate_local_policy(
        &cfg,
        &config,
        &d.path().join("converyn"),
    )
    .unwrap();
}
#[test]
fn observes_repairs_and_verifies_content_and_permissions() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config");
    let policy = Policy {
        file_roots: vec![d.path().into()],
        ..Policy::default()
    };
    let op = operation(&p, b"desired", 0o640);
    let cancel = AtomicBool::new(false);
    let first = reconcile_file(&policy, &op, b"desired", &cancel).unwrap();
    assert!(first.changed && first.verified());
    assert!(first.before.is_none());
    assert!(
        !reconcile_file(&policy, &op, b"desired", &cancel)
            .unwrap()
            .changed
    );
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        reconcile_file(&policy, &op, b"desired", &cancel)
            .unwrap()
            .changed
    );
    fs::write(&p, b"drift").unwrap();
    assert!(
        reconcile_file(&policy, &op, b"desired", &cancel)
            .unwrap()
            .verified()
    );
}
#[test]
fn denies_escape_symlinks_and_preserves_hardlinked_source() {
    let d = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let policy = Policy {
        file_roots: vec![d.path().into()],
        ..Policy::default()
    };
    let cancel = AtomicBool::new(false);
    fs::write(outside.path().join("original"), b"original").unwrap();
    symlink(outside.path(), d.path().join("escape")).unwrap();
    assert!(
        reconcile_file(
            &policy,
            &operation(&d.path().join("escape/original"), b"new", 0o600),
            b"new",
            &cancel
        )
        .is_err()
    );
    symlink(outside.path().join("original"), d.path().join("link")).unwrap();
    assert!(
        reconcile_file(
            &policy,
            &operation(&d.path().join("link"), b"new", 0o600),
            b"new",
            &cancel
        )
        .is_err()
    );
    assert!(
        reconcile_file(
            &policy,
            &operation(&outside.path().join("original"), b"new", 0o600),
            b"new",
            &cancel
        )
        .is_err()
    );
    let p = d.path().join("hardlink");
    fs::hard_link(outside.path().join("original"), &p).unwrap();
    reconcile_file(&policy, &operation(&p, b"new", 0o600), b"new", &cancel).unwrap();
    assert_eq!(
        fs::read(outside.path().join("original")).unwrap(),
        b"original"
    );
}
#[test]
fn receipt_replay_preserves_original_evidence_and_new_intent_repairs_drift() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config");
    let state = d.path().join("journal");
    let policy = Policy {
        file_roots: vec![d.path().into()],
        ..Policy::default()
    };
    let op = operation(&p, b"desired", 0o600);
    let cancel = AtomicBool::new(false);
    let r = execute(&state, "run/work", &policy, &op, b"desired", &cancel, None).unwrap();
    assert_eq!(r.outcome, Outcome::Succeeded);
    fs::write(&p, b"drift").unwrap();
    assert_eq!(
        execute(&state, "run/work", &policy, &op, b"desired", &cancel, None).unwrap(),
        r
    );
    assert_eq!(fs::read(&p).unwrap(), b"drift");
    assert!(
        execute(
            &state,
            "run/work",
            &policy,
            &operation(&p, b"other", 0o600),
            b"other",
            &cancel,
            None
        )
        .is_err()
    );
    assert!(
        execute(&state, "run2/work", &policy, &op, b"desired", &cancel, None)
            .unwrap()
            .native
            .unwrap()
            .verified()
    );
}
#[test]
fn canceled_or_wrong_digest_never_publishes() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("config");
    let policy = Policy {
        file_roots: vec![d.path().into()],
        ..Policy::default()
    };
    let op = operation(&p, b"desired", 0o600);
    assert!(reconcile_file(&policy, &op, b"wrong", &AtomicBool::new(false)).is_err());
    assert!(reconcile_file(&policy, &op, b"desired", &AtomicBool::new(true)).is_err());
    assert!(!p.exists());
    assert!(reconcile_file(&Policy::default(), &op, b"desired", &AtomicBool::new(false)).is_err());
}

#[test]
fn policy_cannot_authorize_its_own_state_or_credentials() {
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("state");
    fs::create_dir(&state).unwrap();
    let policy = Policy {
        file_roots: vec![d.path().into()],
        ..Policy::default()
    };
    assert!(policy.validate(&[&state]).is_err());
    let managed = d.path().join("managed");
    fs::create_dir(&managed).unwrap();
    let policy = Policy {
        file_roots: vec![managed],
        ..Policy::default()
    };
    assert!(policy.validate(&[&state]).is_ok());
}
#[test]
fn service_verification_rejects_starting_failed_or_static_states() {
    use converyn_runtime::executors::service::State;
    let mut s = State {
        executable_sha256: None,
        main_pid: None,
        load: "loaded".into(),
        active: "active".into(),
        enabled: "enabled".into(),
        sub: "running".into(),
    };
    assert!(s.matches(true, true));
    s.active = "activating".into();
    assert!(!s.matches(true, true));
    s.active = "failed".into();
    assert!(!s.matches(false, true));
    s.active = "active".into();
    s.enabled = "static".into();
    assert!(!s.matches(true, false));
    let p = Policy::default();
    assert!(
        converyn_runtime::executors::service::reconcile(
            &p,
            &Operation::Service {
                executable_sha256: None,
                name: "unapproved.service".into(),
                running: true,
                enabled: true
            },
            &AtomicBool::new(false)
        )
        .is_err()
    );
}

#[test]
fn binary_install_streams_verifies_and_preserves_previous_on_corrupt_source() {
    use converyn_runtime::artifacts::source::{ArtifactRef, Location};
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("cache");
    let path = d.path().join("app");
    let bytes = vec![42u8; 1024 * 1024];
    fs::write(&source, &bytes).unwrap();
    let artifact = ArtifactRef {
        sha256: hex::encode(Sha256::digest(&bytes)),
        size: bytes.len() as u64,
        locations: vec![Location {
            source: "private".into(),
            path: "/app".into(),
        }],
    };
    let op = Operation::Binary {
        version_directory: None,
        path: path.to_str().unwrap().into(),
        artifact,
        mode: 0o755,
    };
    let policy = Policy {
        file_roots: vec![d.path().into()],
        ..Policy::default()
    };
    let cancel = AtomicBool::new(false);
    let r = converyn_runtime::executors::binary::reconcile(&policy, &op, &source, &cancel).unwrap();
    assert!(r.verified() && r.changed);
    assert!(
        !converyn_runtime::executors::binary::reconcile(&policy, &op, &source, &cancel)
            .unwrap()
            .changed
    );
    fs::write(&path, b"previous").unwrap();
    fs::write(&source, vec![43u8; bytes.len()]).unwrap();
    assert!(
        converyn_runtime::executors::binary::reconcile(&policy, &op, &source, &cancel).is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"previous");
    assert!(fs::read_dir(d.path()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".converyn-")
    }));
}

#[test]
fn interrupted_package_transaction_is_not_completed_by_matching_version() {
    use converyn_runtime::artifacts::source::{ArtifactRef, Location};
    use converyn_runtime::executors::native::Evidence;
    use converyn_runtime::executors::package::{Evidence as PackageEvidence, Manager};
    use converyn_runtime::executors::runner::{Outcome, Receipt};
    let d = tempfile::tempdir().unwrap();
    let operation = Operation::Package {
        install_only: false,
        manager: Manager::Deb,
        name: "converyn-fixture".into(),
        version: "1.0".into(),
        artifact: ArtifactRef {
            sha256: "a".repeat(64),
            size: 1,
            locations: vec![Location {
                source: "lab".into(),
                path: "/package".into(),
            }],
        },
        timeout_seconds: 60,
    };
    let evidence = Evidence {
        plugin: None,
        operation: operation.clone(),
        before: None,
        after: None,
        service_before: None,
        service_after: None,
        changed: true,
        error: None,
        artifact_size: Some(1),

        reboot: None,
        health: None,

        package: Some(PackageEvidence {
            before: None,
            after: Some("1.0".into()),
            transaction_exit: None,
            diagnostic: String::new(),
        }),
    };
    assert!(!evidence.verified());
    let receipt = Receipt {
        outputs: Default::default(),
        diagnostic: None,
        native: Some(Box::new(evidence)),
        execution: None,
        logs: None,
        operation_id: "package/run".into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        outcome: Outcome::Running,
        exit_code: None,
        boot_id: "original-boot".into(),
    };
    let key = hex::encode(Sha256::digest("package/run"));
    fs::write(
        d.path().join(format!("{key}.json")),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    let recovered = execute(
        d.path(),
        "package/run",
        &Policy::default(),
        &operation,
        &[],
        &AtomicBool::new(false),
        None,
    )
    .unwrap();
    assert_eq!(recovered.outcome, Outcome::NeedsAttention);
    assert!(!recovered.native.unwrap().verified());
}

#[test]
fn versioned_binary_activation_and_rollback_preserve_both_inodes() {
    use converyn_runtime::artifacts::source::{ArtifactRef, Location};
    use std::os::unix::fs::MetadataExt;
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source");
    let target = d.path().join("current");
    let versions = d.path().join("versions");
    let cancel = AtomicBool::new(false);
    let operation = |bytes: &[u8]| Operation::Binary {
        version_directory: Some(versions.to_str().unwrap().into()),
        path: target.to_str().unwrap().into(),
        artifact: ArtifactRef {
            sha256: hex::encode(Sha256::digest(bytes)),
            size: bytes.len() as u64,
            locations: vec![Location {
                source: "local".into(),
                path: "/binary".into(),
            }],
        },
        mode: 0o755,
    };
    fs::write(&source, b"version-one").unwrap();
    let first = operation(b"version-one");
    let first_policy = converyn_runtime::executors::native::managed_policy(&first, None).unwrap();
    assert!(first_policy.file_roots.is_empty());
    assert_eq!(first_policy.file_paths.len(), 2);
    converyn_runtime::executors::binary::reconcile(&first_policy, &first, &source, &cancel)
        .unwrap();
    let inode = fs::metadata(&target).unwrap().ino();
    fs::write(&source, b"version-two").unwrap();
    let second = operation(b"version-two");
    // A grant for one content hash cannot publish another version in the same directory.
    let denied =
        converyn_runtime::executors::binary::reconcile(&first_policy, &second, &source, &cancel)
            .unwrap_err();
    assert!(denied.to_string().contains("outside configured roots"));
    assert_eq!(fs::metadata(&target).unwrap().ino(), inode);
    assert_eq!(fs::read_dir(&versions).unwrap().count(), 1);
    let second_policy = converyn_runtime::executors::native::managed_policy(&second, None).unwrap();
    converyn_runtime::executors::binary::reconcile(&second_policy, &second, &source, &cancel)
        .unwrap();
    assert_ne!(fs::metadata(&target).unwrap().ino(), inode);
    fs::remove_file(&source).unwrap();
    let result =
        converyn_runtime::executors::binary::reconcile(&first_policy, &first, &source, &cancel)
            .unwrap();
    assert!(result.verified());
    assert_eq!(fs::metadata(&target).unwrap().ino(), inode);
    assert_eq!(fs::read_dir(&versions).unwrap().count(), 2);
}
#[test]
fn running_executable_is_verified_independently_of_replaced_disk_path() {
    let d = tempfile::tempdir().unwrap();
    let executable = d.path().join("app");
    fs::copy("/usr/bin/sleep", &executable).unwrap();
    let mut child = std::process::Command::new(&executable)
        .arg("30")
        .spawn()
        .unwrap();
    let cancel = AtomicBool::new(false);
    let before =
        converyn_runtime::executors::service::executable_digest(child.id(), &cancel).unwrap();
    let replacement = d.path().join("replacement");
    fs::copy("/usr/bin/true", &replacement).unwrap();
    fs::rename(&replacement, &executable).unwrap();
    let after =
        converyn_runtime::executors::service::executable_digest(child.id(), &cancel).unwrap();
    let disk = hex::encode(Sha256::digest(fs::read(&executable).unwrap()));
    let _ = child.kill();
    child.wait().unwrap();
    assert_eq!(before, after);
    assert_ne!(after, disk);
}
