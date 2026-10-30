use converyn_core::model::domain::Operation;
use converyn_runtime::executors::native::{Evidence, Policy};
use converyn_runtime::executors::runner::{Outcome, Receipt};
#[test]
fn explicit_platform_identity_rejects_writable_or_symlinked_inputs() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("hardware");
    std::fs::write(&p, b"operator-inventory-uuid").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    let first = converyn_runtime::executors::host::hardware(Some(&p)).unwrap();
    assert!(first.is_some());
    assert_eq!(
        first,
        converyn_runtime::executors::host::hardware(Some(&p)).unwrap()
    );
    let link = d.path().join("alias");
    symlink(&p, &link).unwrap();
    assert!(converyn_runtime::executors::host::hardware(Some(&link)).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(converyn_runtime::executors::host::hardware(Some(&p)).is_err());
}
fn prior(operation: &Operation, requested_ms: u64) -> Receipt {
    let mut evidence = Evidence::empty(operation);
    evidence.reboot = Some(converyn_runtime::executors::host::Reboot {
        before_boot: "previous-boot".into(),
        after_boot: None,
        kernel: None,
        requested_ms,
    });
    Receipt {
        outputs: Default::default(),
        diagnostic: None,
        operation_id: "reboot/work".into(),
        script_sha256: String::new(),
        timeout_seconds: 0,
        outcome: Outcome::Running,
        exit_code: None,
        boot_id: "previous-boot".into(),
        native: Some(Box::new(evidence)),
        execution: None,
        logs: None,
    }
}
#[test]
fn reboot_recovers_from_boot_and_kernel_evidence_without_resending() {
    let d = tempfile::tempdir().unwrap();
    let operation = Operation::Reboot {
        requested: true,
        recovery_timeout_seconds: 60,
        expected_kernel: Some(converyn_runtime::executors::host::kernel().unwrap()),
    };
    let receipt = converyn_runtime::executors::host::reboot(
        &d.path().join("receipt"),
        "reboot/work",
        &Policy::default(),
        &operation,
        Some(prior(
            &operation,
            converyn_runtime::executors::host::now_ms(),
        )),
    )
    .unwrap();
    assert_eq!(receipt.outcome, Outcome::Succeeded);
    assert!(receipt.native.unwrap().verified());
    assert_eq!(
        receipt.boot_id,
        converyn_runtime::executors::host::boot().unwrap()
    );
    let wrong = Operation::Reboot {
        requested: true,
        recovery_timeout_seconds: 60,
        expected_kernel: Some("not-the-running-kernel".into()),
    };
    let r = converyn_runtime::executors::host::reboot(
        &d.path().join("wrong"),
        "reboot/work",
        &Policy::default(),
        &wrong,
        Some(prior(&wrong, converyn_runtime::executors::host::now_ms())),
    )
    .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    let late = converyn_runtime::executors::host::reboot(
        &d.path().join("late"),
        "reboot/work",
        &Policy::default(),
        &operation,
        Some(prior(&operation, 1)),
    )
    .unwrap();
    assert_eq!(late.outcome, Outcome::NeedsAttention);
}
#[test]
fn reboot_is_denied_by_default_and_unchanged_boot_is_not_success() {
    let d = tempfile::tempdir().unwrap();
    let operation = Operation::Reboot {
        requested: true,
        recovery_timeout_seconds: 60,
        expected_kernel: None,
    };
    let r = converyn_runtime::executors::host::reboot(
        &d.path().join("denied"),
        "reboot/work",
        &Policy::default(),
        &operation,
        None,
    )
    .unwrap();
    assert_eq!(r.outcome, Outcome::Failed);
    let mut pending = prior(&operation, converyn_runtime::executors::host::now_ms());
    pending
        .native
        .as_mut()
        .unwrap()
        .reboot
        .as_mut()
        .unwrap()
        .before_boot = converyn_runtime::executors::host::boot().unwrap();
    pending.boot_id = converyn_runtime::executors::host::boot().unwrap();
    let r = converyn_runtime::executors::host::reboot(
        &d.path().join("pending"),
        "reboot/work",
        &Policy::default(),
        &operation,
        Some(pending),
    )
    .unwrap();
    assert_eq!(r.outcome, Outcome::Running);
}
#[test]
fn application_health_retries_readiness_without_controller_credentials() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        for status in [503, 200] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]).to_lowercase();
            assert!(
                !request.contains("authorization:") && !request.contains("x-converyn-cluster:")
            );
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            )
            .unwrap();
        }
    });
    let op = Operation::HttpCheck {
        url: format!("{origin}/ready"),
        expected_status: 200,
        body_sha256: Some(hex::encode(Sha256::digest(b"ok"))),
        timeout_seconds: 5,
    };
    let policy = Policy {
        health_origins: vec![origin],
        ..Policy::default()
    };
    let evidence = converyn_runtime::executors::health::check(
        &policy,
        &op,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert!(evidence.verified());
    task.join().unwrap();
    assert!(converyn_runtime::executors::health::validate_url("http://169.254.169.254/").is_err());
    assert!(
        converyn_runtime::executors::health::validate_url("https://user:secret@example.com/")
            .is_err()
    );
}
