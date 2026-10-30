use converyn_core::model::domain::Operation;
use converyn_runtime::artifacts::source::{ArtifactRef, Location};
use converyn_runtime::executors::native::{Policy, execute};
use converyn_runtime::executors::package::Manager;
use converyn_runtime::executors::runner::Outcome;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::atomic::AtomicBool};
#[test]
#[ignore = "requires the isolated dpkg fixture image, never run against host package database"]
fn dpkg_real_install_dedup_upgrade_and_explicit_rollback() {
    assert_eq!(
        std::env::var("CONVERYN_PACKAGE_TEST").unwrap(),
        "isolated-dpkg-fixture"
    );
    assert_eq!(
        std::fs::read_to_string("/converyn-test-image").unwrap(),
        "isolated-dpkg-fixture\n"
    );
    let state = tempfile::tempdir().unwrap();
    let policy = Policy {
        packages: vec!["converyn-fixture".into()],
        ..Policy::default()
    };
    let mut receipts = Vec::new();
    for (id, version) in [
        ("first", "1.0"),
        ("first", "1.0"),
        ("upgrade", "2.0"),
        ("rollback", "1.0"),
    ] {
        let path = format!("/fixtures/converyn-fixture-{version}.deb");
        let data = std::fs::read(&path).unwrap();
        let operation = Operation::Package {
            install_only: false,
            manager: Manager::Deb,
            name: "converyn-fixture".into(),
            version: version.into(),
            artifact: ArtifactRef {
                sha256: hex::encode(Sha256::digest(&data)),
                size: data.len() as u64,
                locations: vec![Location {
                    source: "fixture".into(),
                    path: "/artifact".into(),
                }],
            },
            timeout_seconds: 60,
        };
        let receipt = execute(
            state.path(),
            id,
            &policy,
            &operation,
            &[],
            &AtomicBool::new(false),
            Some(Path::new(&path)),
        )
        .unwrap();
        assert_eq!(receipt.outcome, Outcome::Succeeded, "{receipt:?}");
        assert!(receipt.native.as_ref().unwrap().verified());
        receipts.push(receipt);
    }
    assert_eq!(receipts[0], receipts[1]);
    let effects = std::fs::read_to_string("/var/lib/converyn-fixture/install-events").unwrap();
    assert_eq!(effects, "1.0\n2.0\n1.0\n");
    assert_eq!(
        std::fs::read_to_string("/opt/converyn-fixture/version").unwrap(),
        "1.0\n"
    );
    std::fs::write("/results/dpkg.json",serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"environment":std::fs::read_to_string("/etc/os-release").unwrap_or("isolated Ubuntu 22.04 dpkg userspace fixture".into()),"qualification":"container package-manager behavior; not a full VM install qualification","receipts":receipts,"postinstall_events":effects.lines().collect::<Vec<_>>()})).unwrap()).unwrap();
}
