//! Real browser workflow submission through an isolated client-certificate relay.
//! The relay supplies the fixture's Alice certificate; backend mTLS and person
//! attribution are exercised by workflow_e2e.py, without production mutations.
#[test]
#[ignore = "requires built full binary, explicit Chrome, loopback and isolated process execution"]
fn embedded_console_polling_and_details() {
    let chrome = std::env::var("CONVERYN_CHROME").expect("set CONVERYN_CHROME");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("CONVERYN_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/debug/converyn"));
    let report = std::env::var_os("CONVERYN_BROWSER_REPORT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("converyn-workflow-browser.json"));
    assert!(
        std::process::Command::new("python3")
            .arg(root.join("scripts/tests/e2e/workflow_e2e.py"))
            .arg("--binary")
            .arg(binary)
            .arg("--chrome")
            .arg(chrome)
            .arg("--browser-report")
            .arg(report)
            .status()
            .unwrap()
            .success()
    );
}
