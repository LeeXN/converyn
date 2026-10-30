//! Three local processes test actual consensus/failover and full cold-audit
//! backup recovery. This is not a claim of three independent failure domains.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};
#[tokio::test]
async fn fixture() {
    let Ok(config) = std::env::var("CONVERYN_HA_PRODUCT") else {
        return;
    };
    let v: Value = serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
    converyn_runtime::controller::server::serve_settings(
        converyn_runtime::controller::server::Settings {
            output_keyring: None,
            id: v["id"].as_u64().unwrap(),
            listen: v["addr"].as_str().unwrap().parse().unwrap(),
            cluster: "ha-product".into(),
            token: "ha-token-abcdefghijklmnopqrstuvwxyz".into(),
            data_dir: v["data"].as_str().unwrap().into(),
            auth: None,
            tls: None,
            issuer: None,
            plugin_connections: Default::default(),
            log_store: Some(converyn_runtime::log_store::Config::Filesystem {
                directory: v["logs"].as_str().unwrap().into(),
                shared: true,
                retention_days: 30,
            }),
        },
    )
    .await
    .unwrap();
}
struct Member {
    child: Option<Child>,
    base: String,
}
impl Drop for Member {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
fn start(root: &Path, index: u64, logs: &Path, data: PathBuf) -> Member {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);
    let cfg = root.join(format!("member-{index}.json"));
    std::fs::write(
        &cfg,
        serde_json::to_vec(&json!({"id":index,"addr":addr,"logs":logs,"data":data})).unwrap(),
    )
    .unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture", "--nocapture"])
        .env("CONVERYN_HA_PRODUCT", cfg)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    Member {
        child: Some(child),
        base: format!("http://{addr}"),
    }
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}
fn request(c: &reqwest::Client, base: &str, path: &str) -> reqwest::RequestBuilder {
    c.get(format!("{base}{path}"))
        .bearer_auth("ha-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "ha-product")
}
async fn ready(c: &reqwest::Client, members: &[Member]) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let mut all = true;
            for m in members {
                match c.get(format!("{}/health/live", m.base)).send().await {
                    Ok(response) => match response.json::<Value>().await {
                        Ok(v) if v["log_storage"]["id"].is_string() => {}
                        _ => all = false,
                    },
                    Err(_) => all = false,
                }
            }
            if all {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap()
}
async fn leader(c: &reqwest::Client, members: &[Member]) -> usize {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            for (i, m) in members.iter().enumerate() {
                if m.child.is_none() {
                    continue;
                }
                if let Ok(r) = request(c, &m.base, "/api/v1/controlplane/metrics")
                    .send()
                    .await
                    && let Ok(v) = r.json::<Value>().await
                    && v["metrics"]["state"] == "Leader"
                {
                    return i;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn shared_storage_is_probed_and_cold_audit_survives_leader_loss_and_three_member_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let c = client();
    let logs = tmp.path().join("shared");
    let mut members = (1..=3)
        .map(|i| start(tmp.path(), i, &logs, tmp.path().join(format!("data-{i}"))))
        .collect::<Vec<_>>();
    ready(&c, &members).await;
    let mut config = serde_json::Map::new();
    for (i, m) in members.iter().enumerate() {
        config.insert(
            (i + 1).to_string(),
            json!({"addr":m.base.strip_prefix("http://").unwrap()}),
        );
    }
    let r = c
        .post(format!("{}/api/v1/controlplane/init", members[0].base))
        .bearer_auth("ha-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "ha-product")
        .json(&config)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let old_leader = leader(&c, &members).await;
    let follower = (old_leader + 1) % 3;
    let command = json!({"action":"create_node_group","expected_revision":0,"group":{"uid":"durable","name":"durable","revision":1,"node_uids":[]}});
    let response = c
        .post(format!(
            "{}/api/v1/node-groups/commands",
            members[follower].base
        ))
        .bearer_auth("ha-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "ha-product")
        .header("x-request-id", "follower-origin")
        .json(&command)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["x-request-id"], "follower-origin");
    assert_eq!(response.json::<Value>().await.unwrap()["ok"], true);
    let workflow = json!({"workflow":{"uid":"durable-flow","name":"Durable workflow","targets":{"qa":{"node_group_uid":"durable","hosts":"all"}},"playbook":{"api_version":"converyn/v1","name":"durable","steps":[{"id":"check","phase":"verify","scope":"per_node","operation":{"kind":"http_check","url":"https://example.invalid/","expected_status":200,"timeout_seconds":1}}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}]}},"expected_revision":0});
    let r = c
        .post(format!(
            "{}/api/v1/workflows/register",
            members[follower].base
        ))
        .bearer_auth("ha-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "ha-product")
        .json(&workflow)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 409);
    let mut old = members[old_leader].child.take().unwrap();
    old.kill().unwrap();
    old.wait().unwrap();
    let new_leader = leader(&c, &members).await;
    assert_ne!(old_leader, new_leader);
    let v: Value = request(&c, &members[new_leader].base, "/api/v1/node-groups")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["node_groups"]["durable"]["uid"], "durable");
    let backup = request(&c, &members[new_leader].base, "/api/v1/controlplane/backup")
        .send()
        .await
        .unwrap();
    assert_eq!(backup.status(), 200);
    let path = tmp.path().join("backup.cvb");
    std::fs::write(&path, backup.bytes().await.unwrap()).unwrap();
    let (checksum, cluster) = converyn_runtime::storage::backup::inspect(&path).unwrap();
    assert_eq!(cluster, "ha-product");
    assert_eq!(checksum.len(), 64);
    drop(members);
    let mut restored = Vec::new();
    for i in 1..=3 {
        let data = tmp.path().join(format!("restore-{i}"));
        converyn_runtime::storage::backup::restore(&path, &data, i, true)
            .await
            .unwrap();
        restored.push(start(tmp.path(), i, &logs, data));
    }
    ready(&c, &restored).await;
    let mut config = serde_json::Map::new();
    for (i, m) in restored.iter().enumerate() {
        config.insert(
            (i + 1).to_string(),
            json!({"addr":m.base.strip_prefix("http://").unwrap()}),
        );
    }
    let r = c
        .post(format!("{}/api/v1/controlplane/init", restored[0].base))
        .bearer_auth("ha-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "ha-product")
        .json(&config)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let leader = leader(&c, &restored).await;
    let v: Value = request(&c, &restored[leader].base, "/api/v1/node-groups")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["node_groups"]["durable"]["uid"], "durable");
    let workflows: Value = request(&c, &restored[leader].base, "/api/v1/workflows")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(workflows["workflows"].as_array().unwrap().is_empty());
    let audit: Value = request(&c, &restored[leader].base, "/api/v1/audit")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!audit["records"].as_array().unwrap().is_empty());
}
#[tokio::test]
async fn separate_disks_with_the_same_shared_flag_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let c = client();
    let members = (1..=3)
        .map(|i| {
            start(
                tmp.path(),
                i,
                &tmp.path().join(format!("separate-{i}")),
                tmp.path().join(format!("data-{i}")),
            )
        })
        .collect::<Vec<_>>();
    ready(&c, &members).await;
    let mut nodes = serde_json::Map::new();
    for (i, m) in members.iter().enumerate() {
        nodes.insert(
            (i + 1).to_string(),
            json!({"addr":m.base.strip_prefix("http://").unwrap()}),
        );
    }
    let r = c
        .post(format!("{}/api/v1/controlplane/init", members[0].base))
        .bearer_auth("ha-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "ha-product")
        .json(&nodes)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 422);
}
