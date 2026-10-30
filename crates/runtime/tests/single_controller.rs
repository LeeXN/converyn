//! Exercise the real HTTP/Raft/SQLite path in a disposable child process. Killing
//! the child is intentional crash recovery; no signal or service on the host is touched.
use serde_json::{Value, json};
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};

#[tokio::test]
async fn controller_fixture() {
    let Ok(directory) = std::env::var("CONVERYN_SINGLE_TEST_DIR") else {
        return;
    };
    let address = std::env::var("CONVERYN_SINGLE_TEST_ADDR")
        .unwrap()
        .parse()
        .unwrap();
    converyn_runtime::controller::server::serve(
        1,
        address,
        "single-test".into(),
        "test-token-abcdefghijklmnopqrstuvwxyz".into(),
        std::path::Path::new(&directory),
    )
    .await
    .unwrap();
}

struct Controller(Child);
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(directory: &std::path::Path, address: &str) -> Controller {
    Controller(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "controller_fixture", "--nocapture"])
            .env("CONVERYN_SINGLE_TEST_DIR", directory)
            .env("CONVERYN_SINGLE_TEST_ADDR", address)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}
async fn wait_live(client: &reqwest::Client, base: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if client
                .get(format!("{base}/health/live"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("child controller did not start");
}
fn request(
    client: &reqwest::Client,
    base: &str,
    method: reqwest::Method,
    path: &str,
) -> reqwest::RequestBuilder {
    client
        .request(method, format!("{base}{path}"))
        .bearer_auth("test-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "single-test")
}

#[tokio::test]
async fn one_member_initializes_commits_and_recovers_without_weakening_validation() {
    let directory = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let base = format!("http://{address}");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let controller = start(directory.path(), &address);
    wait_live(&client, &base).await;
    let mut membership = json!({"1":{"addr":address},"2":{"addr":"127.0.0.1:1"}});
    let invalid = request(
        &client,
        &base,
        reqwest::Method::POST,
        "/api/v1/controlplane/init",
    )
    .json(&membership)
    .send()
    .await
    .unwrap();
    assert_eq!(invalid.status(), 400);
    membership.as_object_mut().unwrap().remove("2");
    let initialized = request(
        &client,
        &base,
        reqwest::Method::POST,
        "/api/v1/controlplane/init",
    )
    .json(&membership)
    .send()
    .await
    .unwrap();
    assert_eq!(initialized.status(), 200);
    assert!(initialized.headers().contains_key("x-request-id"));
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if request(&client, &base, reqwest::Method::GET, "/health/ready")
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let command = json!({"action":"create_node_group","expected_revision":0,"group":{"uid":"example","name":"example","revision":1,"node_uids":[]}});
    let response = request(
        &client,
        &base,
        reqwest::Method::POST,
        "/api/v1/node-groups/commands",
    )
    .header("x-request-id", "single-controller-regression")
    .json(&command)
    .send()
    .await
    .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["x-request-id"],
        "single-controller-regression"
    );
    assert_eq!(response.json::<Value>().await.unwrap()["ok"], true);
    let metrics: Value = request(
        &client,
        &base,
        reqwest::Method::GET,
        "/api/v1/controlplane/metrics",
    )
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(metrics["writes"]["limit"], 32);
    assert_eq!(metrics["writes"]["ordinary_limit"], 24);
    assert!(metrics["writes"]["committed"].as_u64().unwrap() > 0);
    drop(controller);
    let _restarted = start(directory.path(), &address);
    wait_live(&client, &base).await;
    let catalog = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let response = request(&client, &base, reqwest::Method::GET, "/api/v1/node-groups")
                .send()
                .await
                .unwrap();
            if response.status().is_success() {
                break response.json::<Value>().await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(catalog["node_groups"]["example"]["uid"], "example");
    let replay: Value = request(
        &client,
        &base,
        reqwest::Method::POST,
        "/api/v1/controlplane/init",
    )
    .json(&membership)
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(replay["replayed"], true);
    // A non-loopback deployment still requires mTLS; single member is not a
    // shortcut into the local prototype protocol.
    let unsafe_directory = directory.path().join("unsafe");
    assert!(
        converyn_runtime::controller::server::serve(
            1,
            "0.0.0.0:0".parse().unwrap(),
            "single-test".into(),
            "a".repeat(32),
            &unsafe_directory
        )
        .await
        .is_err()
    );
    assert!(!unsafe_directory.exists());
}

#[test]
fn deployment_bundle_accepts_one_member_and_rejects_two() {
    let d = tempfile::tempdir().unwrap();
    converyn_runtime::controller::initialization::create(
        &d.path().join("one"),
        "single-test",
        &["1=127.0.0.1:8443".into()],
        "team",
        "hosts",
        &[],
    )
    .unwrap();
    let profile: Value = serde_json::from_slice(
        &std::fs::read(d.path().join("one/controller-1/server.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(profile["id"], 1);
    let members: Value =
        serde_json::from_slice(&std::fs::read(d.path().join("one/members.json")).unwrap()).unwrap();
    assert_eq!(members.as_object().unwrap().len(), 1);
    assert_eq!(members["1"]["addr"], "127.0.0.1:8443");
    assert!(
        converyn_runtime::controller::initialization::create(
            &d.path().join("two"),
            "single-test",
            &["1=127.0.0.1:8443".into(), "2=127.0.0.1:8444".into()],
            "team",
            "hosts",
            &[]
        )
        .is_err()
    );
    assert!(!d.path().join("two").exists());
}
