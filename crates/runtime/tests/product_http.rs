//! Real mTLS HTTP fixture with independent human/CI/Node credentials.
mod support;
use converyn_core::orchestration::machine::{State, Status};
use converyn_runtime::{
    controller::initialization::{self, ServerProfile},
    credentials::{
        pki,
        security::{Auth, Tls},
    },
    storage::store::Store,
};
use serde_json::{Value, json};
static EVIDENCE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Child, Command as Process, Stdio},
    time::Duration,
};
#[tokio::test]
async fn fixture() {
    let Ok(path) = std::env::var("CONVERYN_PRODUCT_HTTP_PROFILE") else {
        return;
    };
    let profile: ServerProfile = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let path = Path::new(&path);
    converyn_runtime::controller::server::serve_settings(
        profile.settings(path.parent().unwrap()).unwrap(),
    )
    .await
    .unwrap();
}
struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(profile: &Path) -> Server {
    Server(
        Process::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture", "--nocapture"])
            .env("CONVERYN_PRODUCT_HTTP_PROFILE", profile)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}
fn client(root: &Path, name: &str) -> reqwest::Client {
    Tls {
        cert: root.join(name).join("tls.crt"),
        key: root.join(name).join("tls.key"),
        ca: root.join("authority/ca.crt"),
    }
    .client()
    .unwrap()
}
async fn call(
    c: &reqwest::Client,
    base: &str,
    token: &str,
    path: &str,
    mut body: Option<Value>,
) -> (u16, Value) {
    let mut r = if body.is_some() {
        c.post(format!("{base}{path}"))
    } else {
        c.get(format!("{base}{path}"))
    };
    r = r
        .bearer_auth(token)
        .header("x-converyn-cluster", "product")
        .header(
            "x-converyn-protocol",
            converyn_runtime::controller::network::PROTOCOL_VERSION,
        );
    if token == "node-token-abcdefghijklmnopqrstuvwxyz" {
        let hardware = converyn_core::model::domain::digest(&"node");
        r = r
            .header("x-converyn-hardware", &hardware)
            .header("x-converyn-session", "session");
        if path.ends_with("/heartbeat")
            && let Some(value) = body.as_mut()
        {
            value["hardware_sha256"] = json!(hardware);
        }
    }
    if let Some(body) = &body {
        r = r.json(body)
    }
    let response = r.send().await.unwrap();
    let status = response.status().as_u16();
    assert!(response.headers().contains_key("x-request-id"));
    let value: Value = response.json().await.unwrap();
    if let Ok(evidence_path) = std::env::var("CONVERYN_API_EVIDENCE")
        && !path.split('?').next().unwrap_or(path).ends_with("/inputs")
    {
        let _lock = EVIDENCE_LOCK.lock().unwrap();
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(evidence_path)
            .unwrap();
        writeln!(file,"{}",json!({"method":if body.is_some(){"post"}else{"get"},"path":path,"status":status,"request":body,"response":value})).unwrap();
    }
    (status, value)
}
fn seed() -> (State, String, String) {
    let content = "redacted output\n";
    let sha = hex::encode(Sha256::digest(content));
    let mut s = support::state();
    let input=serde_json::from_value::<converyn_core::model::domain::PlanInput>(json!({"project":"team","owner_uid":"app","batch_size":1,"playbook":{"api_version":"converyn/v1","name":"fixture","steps":[{"id":"script","phase":"rollout","scope":"per_node","operation":{"kind":"exec","script_sha256":sha,"timeout_seconds":30},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"node","hostname":"node","address":"192.0.2.1"}],"groups":{"worker":["node"]}},"hosts":{"node_group_uid":"hosts","revision":1,"node_uids":["node"]}})).unwrap();
    let plan = input.compile().unwrap();
    let work = plan.work[0].id.clone();
    s.apply(converyn_core::orchestration::machine::Command::PutScript {
        sha256: sha.clone(),
        content: content.into(),
    });
    assert!(
        s.apply(converyn_core::orchestration::machine::Command::CreateRun {
            output_key: None,
            run_id: "run".into(),
            idempotency_key: "run".into(),
            plan: Box::new(plan)
        })
        .ok
    );
    let run = std::sync::Arc::make_mut(s.runs.get_mut("run").unwrap());
    run.status = Status::Succeeded;
    let a = std::sync::Arc::make_mut(run.attempts.get_mut(&work).unwrap());
    a.status = Status::Succeeded;
    a.grant = Some("grant".into());
    a.receipt=Some(serde_json::from_value(json!({"operation_id":format!("run/{work}"),"script_sha256":sha,"timeout_seconds":30,"outcome":"succeeded","exit_code":0,"boot_id":"boot","logs":{"stdout":{"stored_bytes":content.len(),"seen_bytes":content.len(),"sha256":sha,"tail":content,"truncated":false},"stderr":{"stored_bytes":0,"seen_bytes":0,"sha256":hex::encode(Sha256::digest(b"")),"tail":"","truncated":false}}})).unwrap());
    s.execution_locks.clear();
    (s, work, sha)
}
#[tokio::test]
async fn personal_credentials_errors_and_log_roundtrip_over_real_mtls() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("bundle");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    initialization::create(
        &root,
        "product",
        &[format!("1={address}")],
        "team",
        "hosts",
        &[],
    )
    .unwrap();
    let authority: pki::Authority = serde_json::from_slice(
        &serde_json::to_vec(
            &json!({"cert":root.join("authority/ca.crt"),"key":root.join("authority/ca.key")}),
        )
        .unwrap(),
    )
    .unwrap();
    let alice = authority.issue(&root.join("alice"), "alice", &[]).unwrap();
    let _bob = authority.issue(&root.join("bob"), "bob", &[]).unwrap();
    let node = authority.issue(&root.join("node"), "node", &[]).unwrap();
    let profile_path = root.join("controller-1/server.json");
    let mut profile: ServerProfile =
        serde_json::from_slice(&std::fs::read(&profile_path).unwrap()).unwrap();
    profile.log_store = Some(converyn_runtime::log_store::Config::Filesystem {
        directory: tmp.path().join("logs"),
        shared: false,
        retention_days: 30,
    });
    std::fs::write(&profile_path, serde_json::to_vec(&profile).unwrap()).unwrap();
    let auth_path = root.join("controller-1/auth.json");
    let mut auth: Auth = serde_json::from_slice(&std::fs::read(&auth_path).unwrap()).unwrap();
    let node_token = "node-token-abcdefghijklmnopqrstuvwxyz";
    auth.agents.insert("node".into(), node_token.into());
    auth.certificates
        .insert("node:node".into(), [node.fingerprint].into());
    std::fs::write(&auth_path, serde_json::to_vec(&auth).unwrap()).unwrap();
    let (mut state, work, sha) = seed();
    let pem = std::fs::read_to_string(root.join("node/tls.crt")).unwrap();
    let info = pki::info(&pem).unwrap();
    let enrolled = std::sync::Arc::make_mut(state.registry.nodes.get_mut("node").unwrap());
    enrolled.certificate = converyn_core::identities::certificate::Signed {
        certificate_pem: pem,
        fingerprint: info.fingerprint,
        expires_ms: info.expires_ms,
    };
    enrolled.token_sha256 = hex::encode(Sha256::digest(node_token));
    state.observations.insert("node".into(),serde_json::from_value(json!({"boot_id":"boot","session_id":"session","observed_ms":1,"hardware_sha256":converyn_core::model::domain::digest(&"node")})).unwrap());

    let live_content = "live-output-中\n".repeat(1000);
    let live_sha = hex::encode(Sha256::digest(live_content.as_bytes()));
    let plan = (*state.runs["run"].plan).clone();
    assert!(
        state
            .apply(converyn_core::orchestration::machine::Command::CreateRun {
                output_key: None,
                run_id: "live-run".into(),
                idempotency_key: "live-run".into(),
                plan: Box::new(plan)
            })
            .ok
    );
    {
        let run = std::sync::Arc::make_mut(state.runs.get_mut("live-run").unwrap());
        run.status = Status::Running;
        let a = std::sync::Arc::make_mut(run.attempts.get_mut(&work).unwrap());
        a.status = Status::Running;
        a.grant = Some("live-grant".into());
        a.boot_id = Some("boot".into());
        a.session_id = Some("session".into());
        a.last_sequence = 1;
    }

    let store = Store::open(&root.join("controller-1/data"), 1).unwrap();
    store.bind_cluster("product").await.unwrap();
    store
        .restore_business_state(state, "fixture")
        .await
        .unwrap();
    drop(store);
    let _server = start(&profile_path);
    let base = format!("https://{address}");
    let admin = client(&root, "admin");
    let admin_token = std::fs::read_to_string(root.join("admin/token")).unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(response) = admin.get(format!("{base}/health/live")).send().await {
                let live: Value = response.json().await.unwrap();
                // The listener starts before the asynchronous log backend. Initialization
                // correctly rejects an unready backend, so wait for the required capability.
                if live["log_storage"]["id"].is_string() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        call(
            &admin,
            &base,
            &admin_token,
            "/api/v1/controlplane/init",
            Some(json!({"1":{"addr":address}}))
        )
        .await
        .0,
        200
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if call(&admin, &base, &admin_token, "/health/ready", None)
                .await
                .0
                == 200
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let (status, e) = call(&admin, &base, "wrong", "/api/v1/runs", None).await;
    assert_eq!(status, 401);
    for field in ["code", "message", "retryable", "request_id"] {
        assert!(e.get(field).is_some())
    }
    let a_token = "alice-token-abcdefghijklmnopqrstuvwxyz";
    let now = converyn_runtime::executors::host::now_ms();
    let c = converyn_core::identities::personnel::Credential {
        uid: "credential-alice".into(),
        identity_uid: "user-alice".into(),
        certificate_pem: alice.certificate_pem.clone(),
        fingerprint: alice.fingerprint.clone(),
        token_sha256: hex::encode(Sha256::digest(a_token)),
        not_before_ms: pki::info(&alice.certificate_pem).unwrap().not_before_ms,
        expires_ms: alice.expires_ms,
        revoked_ms: None,
        overlap_until_ms: None,
    };
    let change = converyn_core::identities::personnel::Change::Create {
        super_admin: true,
        identity: converyn_core::identities::personnel::Identity {
            uid: "user-alice".into(),
            name: "Alice".into(),

            disabled: false,
            revision: 1,
        },
        credential: c,
        request_id: "create-alice".into(),
        now_ms: now,
    };
    assert_eq!(
        call(
            &admin,
            &base,
            &admin_token,
            "/api/v1/identities",
            Some(serde_json::to_value(change).unwrap())
        )
        .await
        .0,
        200
    );
    let alice = client(&root, "alice");
    let bob_client = client(&root, "bob");
    assert_eq!(
        call(&bob_client, &base, a_token, "/api/v1/runs/run", None)
            .await
            .0,
        401
    );
    assert_eq!(
        call(&alice, &base, a_token, "/api/v1/runs/run", None)
            .await
            .0,
        200
    );
    let node_client = client(&root, "node");
    let old_agent = node_client
        .get(format!("{base}/api/v1/agents/node/log-storage"))
        .bearer_auth(node_token)
        .header("x-converyn-cluster", "product")
        .send()
        .await
        .unwrap();
    assert_eq!(old_agent.status(), 426);

    assert_eq!(
        call(&node_client, &base, node_token, "/api/v1/runs/run", None)
            .await
            .0,
        403
    );
    assert_eq!(
        call(
            &node_client,
            &base,
            node_token,
            "/api/v1/agents/other/work",
            None
        )
        .await
        .0,
        403
    );
    let inventory = json!({"collected_ms":1,"data":{"hostname":"asset-test","serial":"asset-only"},"errors":{"gpu":"permission denied fixture"}});
    let (status,_) = call(&node_client,&base,node_token,"/api/v1/agents/node/heartbeat",Some(json!({"boot_id":"boot","session_id":"session","inventory":inventory,"inventory_checked_ms":2}))).await;
    assert_eq!(status, 200);
    let (status, n) = call(&alice, &base, a_token, "/api/v1/nodes/node", None).await;
    assert_eq!(status, 200);
    assert_eq!(n["nodes"]["node"]["inventory"], inventory);
    assert_eq!(
        call(
            &node_client,
            &base,
            node_token,
            "/api/v1/agents/node/heartbeat",
            Some(json!({"boot_id":"boot","session_id":"session"}))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(&alice, &base, a_token, "/api/v1/nodes/node", None)
            .await
            .1["nodes"]["node"]["inventory"],
        inventory
    );
    let live_path = format!("/api/v1/agents/node/logs/live-run/{work}/stdout/live");
    let live_data = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        live_content.as_bytes(),
    );
    let body = json!({"grant":"incorrect","offset":0,"data":live_data,"sha256":live_sha});
    assert_eq!(
        call(&node_client, &base, node_token, &live_path, Some(body))
            .await
            .0,
        403
    );
    let data = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        b"redacted output\n",
    );
    let (status, _) = call(
        &node_client,
        &base,
        node_token,
        &format!("/api/v1/agents/node/logs/run/{work}/stdout"),
        Some(json!({"offset":0,"data":data,"sha256":sha,"complete":true})),
    )
    .await;
    assert_eq!(status, 200);
    let read_path = format!("/api/v1/runs/run/logs/{work}/stdout");
    let (status, v) = call(&alice, &base, a_token, &read_path, None).await;
    assert_eq!(status, 200);
    assert_eq!(v["data"], data);
    assert_eq!(v["complete"], true);
    let download = alice
        .get(format!("{base}{read_path}/download"))
        .bearer_auth(a_token)
        .header("x-converyn-cluster", "product")
        .send()
        .await
        .unwrap();
    assert_eq!(download.status(), 200);
    assert_eq!(
        download.bytes().await.unwrap().as_ref(),
        b"redacted output\n"
    );
    assert_eq!(
        call(
            &node_client,
            &base,
            node_token,
            &format!("/api/v1/agents/node/logs/run/{work}/stdout"),
            Some(json!({"offset":0,"data":data,"sha256":sha,"complete":true}))
        )
        .await
        .0,
        200
    );
    // The actual Agent uploader consumes its durable queue, publishes both
    // streams and leaves a cursor that survives a worker restart.
    let agent_state = tmp.path().join("agent");
    let key = hex::encode(Sha256::digest(format!("run/{work}")));
    std::fs::create_dir_all(agent_state.join("uploads")).unwrap();
    std::fs::create_dir_all(agent_state.join("executions").join(&key)).unwrap();
    std::fs::write(
        agent_state.join("executions").join(&key).join("stdout.log"),
        b"redacted output\n",
    )
    .unwrap();
    for n in 0..120 {
        std::fs::write(
            agent_state
                .join("uploads")
                .join(format!("past-{n:03}.cursor.json")),
            b"{}",
        )
        .unwrap();
    }
    std::fs::write(agent_state.join("uploads/broken.json"), b"corrupt evidence").unwrap();
    let queue = agent_state.join("uploads").join(format!("{key}.json"));
    std::fs::write(&queue,serde_json::to_vec(&json!({"run_id":"run","work":{"id":work,"operation":{"kind":"exec"}},"receipt":{"logs":{"stdout":{"stored_bytes":16},"stderr":{"stored_bytes":0}}}})).unwrap()).unwrap();
    pki::private(&root.join("node/token"), node_token.as_bytes()).unwrap();
    let cfg:converyn_runtime::nodes::agent::Config=serde_json::from_value(json!({"node_uid":"node","cluster":"product","servers":[base],"token_file":root.join("node/token"),"state_dir":agent_state,"tls":{"cert":root.join("node/tls.crt"),"key":root.join("node/tls.key"),"ca":root.join("authority/ca.crt")}})).unwrap();
    let upload_api = converyn_runtime::nodes::agent::Client {
        preferred: Default::default(),
        fingerprint: Default::default(),
        hardware: Some(converyn_core::model::domain::digest(&"node")),
        session: Some("session".into()),
        http: std::sync::Arc::new(std::sync::RwLock::new(node_client.clone())),
        servers: vec![base.clone()],
        token: node_token.into(),
        cluster: "product".into(),
    };
    let live_key = hex::encode(Sha256::digest(format!("live-run/{work}")));
    std::fs::create_dir_all(agent_state.join("executions").join(&live_key)).unwrap();
    std::fs::write(
        agent_state
            .join("executions")
            .join(&live_key)
            .join("stdout.log"),
        live_content.as_bytes(),
    )
    .unwrap();
    std::fs::write(agent_state.join("active.json"),serde_json::to_vec(&json!({"run_id":"live-run","grant":"live-grant","work":{"id":work,"operation":{"kind":"exec"}}})).unwrap()).unwrap();
    let uploader = tokio::spawn(converyn_runtime::nodes::log_upload::run(cfg, upload_api));
    tokio::time::timeout(Duration::from_secs(15), async {
        while queue.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let live_read = format!("/api/v1/runs/live-run/logs/{work}/stdout");
    let live = call(&alice, &base, a_token, &live_read, None).await;
    assert_eq!(live.0, 200);
    assert_eq!(live.1["data"], live_data);
    assert_eq!(live.1["source"], "live_fragment");
    assert_eq!(live.1["verified"], false);
    let continuation = call(
        &alice,
        &base,
        a_token,
        &format!("{live_read}?offset=13"),
        None,
    )
    .await
    .1;
    assert_eq!(
        base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            continuation["data"].as_str().unwrap()
        )
        .unwrap(),
        live_content.as_bytes()[13..]
    );
    assert_eq!(continuation["next_offset"], live_content.len());
    assert_eq!(
        call(
            &alice,
            &base,
            a_token,
            &format!("{live_read}/download"),
            None
        )
        .await
        .0,
        409
    );
    uploader.abort();
    assert_eq!(
        std::fs::read(agent_state.join("uploads/broken.invalid")).unwrap(),
        b"corrupt evidence"
    );
    let receipt = json!({"operation_id":format!("live-run/{work}"),"script_sha256":sha,"timeout_seconds":30,"outcome":"succeeded","exit_code":0,"boot_id":"boot","logs":{"stdout":{"stored_bytes":live_content.len(),"seen_bytes":live_content.len(),"sha256":live_sha,"tail":"live tail","truncated":false},"stderr":{"stored_bytes":0,"seen_bytes":0,"sha256":hex::encode(Sha256::digest(b"")),"tail":"","truncated":false}}});
    let report=call(&node_client,&base,node_token,"/api/v1/agents/node/commands",Some(json!({"type":"report","run_id":"live-run","work_id":work,"node_uid":"node","grant":"live-grant","sequence":2,"status":"succeeded","receipt":receipt}))).await;
    assert_eq!(report.0, 200, "{:?}", report);
    assert_eq!(report.1["ok"], true, "{:?}", report);
    assert_eq!(
        call(
            &node_client,
            &base,
            node_token,
            &live_path,
            Some(json!({"grant":"live-grant","offset":0,"data":live_data,"sha256":live_sha}))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &node_client,
            &base,
            node_token,
            &format!("/api/v1/agents/node/logs/live-run/{work}/stdout"),
            Some(json!({"offset":0,"data":live_data,"sha256":live_sha,"complete":true}))
        )
        .await
        .0,
        200
    );
    let final_read = call(&alice, &base, a_token, &live_read, None).await;
    assert_eq!(final_read.1["verified"], true);
    assert_eq!(final_read.1["sha256"], live_sha);
    let response = alice
        .get(format!("{base}{live_read}/download"))
        .bearer_auth(a_token)
        .header("x-converyn-cluster", "product")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        live_content.as_bytes()
    );
    let manifest_path = tmp
        .path()
        .join("logs/logs/product")
        .join(converyn_core::model::domain::digest(&(
            "run", &work, "grant",
        )))
        .join("stdout/manifest");
    let manifest: Value = serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
    assert!(
        manifest["expires_ms"].as_u64().unwrap()
            >= converyn_runtime::executors::host::now_ms() + 30 * 86400 * 1000 - 10000
    );

    let cursor: Value = serde_json::from_slice(
        &std::fs::read(
            agent_state
                .join("uploads")
                .join(format!("{key}.cursor.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cursor["stdout_complete"], true);
    assert_eq!(cursor["stderr_complete"], true);
    let (status, observations) = call(
        &alice,
        &base,
        a_token,
        "/api/v1/audit?view=observations&limit=1",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(observations["view"], "observations");
    assert_eq!(observations["records"][0]["type"], "observe_node");
    let (status, activity) = call(
        &alice,
        &base,
        a_token,
        "/api/v1/audit?view=activity&limit=100",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        activity["records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["type"] != "observe_node" || row["ok"] != true)
    );
    assert_eq!(
        call(&alice, &base, a_token, "/api/v1/audit?view=invalid", None)
            .await
            .0,
        400
    );

    let chunk_path = tmp
        .path()
        .join("logs/logs/product")
        .join(converyn_core::model::domain::digest(&(
            "run", &work, "grant",
        )))
        .join("stdout/0000000000000000");
    std::fs::write(&chunk_path, b"corrupted log").unwrap();
    let (status, corrupt) = call(&alice, &base, a_token, &read_path, None).await;
    assert_eq!(status, 503);
    assert_eq!(corrupt["code"], "log_chunk_checksum_mismatch");
    assert_eq!(corrupt["retryable"], false);
    std::fs::write(chunk_path, b"redacted output\n").unwrap();
    let (status, credentials) = call(&alice, &base, a_token, "/api/v1/credentials", None).await;
    assert_eq!(status, 200);
    assert!(credentials["credentials"][0].get("token_sha256").is_none());
    let cid = credentials["credentials"][0]["uid"].as_str().unwrap();
    let (status, certificate) = call(
        &alice,
        &base,
        a_token,
        &format!("/api/v1/credentials/{cid}"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(
        certificate["credential"]["certificate_pem"]
            .as_str()
            .unwrap()
            .contains("BEGIN CERTIFICATE")
    );
    assert!(!certificate.to_string().contains("PRIVATE KEY"));
    assert!(certificate["credential"].get("token_sha256").is_none());
    assert_eq!(
        call(
            &node_client,
            &base,
            node_token,
            &format!("/api/v1/credentials/{cid}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &alice,
            &base,
            a_token,
            "/api/v1/credentials/not-present",
            None
        )
        .await
        .0,
        404
    );

    assert!(
        credentials["credentials"][0]
            .get("certificate_pem")
            .is_none()
    );
}
#[tokio::test]
async fn unavailable_s3_does_not_prevent_an_existing_controller_from_restarting() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("bundle");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    initialization::create(
        &root,
        "product",
        &[format!("1={address}")],
        "team",
        "hosts",
        &[],
    )
    .unwrap();
    let profile_path = root.join("controller-1/server.json");
    let base = format!("https://{address}");
    let admin = client(&root, "admin");
    let token = std::fs::read_to_string(root.join("admin/token")).unwrap();
    let initial = start(&profile_path);
    tokio::time::timeout(Duration::from_secs(15), async {
        while admin
            .get(format!("{base}/health/live"))
            .send()
            .await
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        call(
            &admin,
            &base,
            &token,
            "/api/v1/controlplane/init",
            Some(json!({"1":{"addr":address}}))
        )
        .await
        .0,
        200
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        while call(&admin, &base, &token, "/health/ready", None).await.0 != 200 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let authority = pki::Authority {
        cert: root.join("authority/ca.crt"),
        key: root.join("authority/ca.key"),
        leaf_lifetime_seconds: 86400,
    };
    let cert = authority
        .issue_personal(&root.join("operator"), "user-operator")
        .unwrap();
    let info = pki::info(&cert.certificate_pem).unwrap();
    let personal_token = "operator-token-abcdefghijklmnopqrstuvwxyz";
    let change = converyn_core::identities::personnel::Change::Create {
        super_admin: true,
        identity: converyn_core::identities::personnel::Identity {
            uid: "user-operator".into(),
            name: "OP".into(),

            disabled: false,
            revision: 1,
        },
        credential: converyn_core::identities::personnel::Credential {
            uid: "credential-op".into(),
            identity_uid: "user-operator".into(),
            certificate_pem: cert.certificate_pem,
            fingerprint: info.fingerprint,
            token_sha256: hex::encode(Sha256::digest(personal_token)),
            not_before_ms: info.not_before_ms,
            expires_ms: info.expires_ms,
            revoked_ms: None,
            overlap_until_ms: None,
        },
        request_id: "create-op".into(),
        now_ms: converyn_runtime::executors::host::now_ms(),
    };
    assert_eq!(
        call(
            &admin,
            &base,
            &token,
            "/api/v1/identities",
            Some(serde_json::to_value(change).unwrap())
        )
        .await
        .0,
        200
    );
    let admin = client(&root, "operator");
    let token = personal_token.to_string();
    drop(initial);
    pki::private(&root.join("access-id"), b"fixture").unwrap();
    pki::private(&root.join("secret-id"), b"fixture-secret").unwrap();
    let mut profile: ServerProfile =
        serde_json::from_slice(&std::fs::read(&profile_path).unwrap()).unwrap();
    profile.log_store = Some(converyn_runtime::log_store::Config::S3 {
        endpoint: "http://127.0.0.1:1/".into(),
        bucket: "fixture".into(),
        region: "us-east-1".into(),
        prefix: "logs".into(),
        access_key_file: root.join("access-id"),
        secret_key_file: root.join("secret-id"),
        session_token_file: None,
        retention_days: 30,
    });
    std::fs::write(&profile_path, serde_json::to_vec(&profile).unwrap()).unwrap();
    let _restarted = start(&profile_path);
    tokio::time::timeout(Duration::from_secs(15), async {
        while admin
            .get(format!("{base}/health/live"))
            .send()
            .await
            .is_err()
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        while call(&admin, &base, &token, "/health/ready", None).await.0 != 200 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let (status, value) = call(
        &admin,
        &base,
        &token,
        "/api/v1/agents/node/log-storage",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value["enabled"], true);
    assert_eq!(value["available"], false);
    assert_eq!(call(&admin,&base,&token,"/api/v1/node-groups/commands",Some(json!({"action":"create_node_group","expected_revision":0,"group":{"uid":"storage-independent","name":"storage-independent","revision":1,"node_uids":[]}}))).await.0,200);
}
