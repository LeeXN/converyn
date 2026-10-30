mod support;
// Repeatable local capacity baseline. Historical data is generated only inside
// this disposable fixture; no host/production deployment operations execute.
use serde_json::{Value, json};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[tokio::test]
async fn fixture() {
    let Ok(dir) = std::env::var("CONVERYN_CAPACITY_DIR") else {
        return;
    };
    let addr = std::env::var("CONVERYN_CAPACITY_ADDR").unwrap();
    converyn_runtime::controller::server::serve(
        1,
        addr.parse().unwrap(),
        "capacity".into(),
        "capacity-token-abcdefghijklmnopqrstuvwxyz".into(),
        std::path::Path::new(&dir),
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
fn seed(path: &std::path::Path) {
    use converyn_core::orchestration::{
        history::{Change, Kind},
        machine::Command,
    };
    use converyn_runtime::storage::{audit_store, state_rows, store::Store};
    use sha2::{Digest, Sha256};
    let store = Store::open(path, 1).unwrap();
    drop(store);
    let mut db = rusqlite::Connection::open(path.join("state.sqlite3")).unwrap();
    let tx = db.transaction().unwrap();
    let mut state = crate::support::state();
    for i in 0..300 {
        state.observations.insert(
            format!("node-{i:03}"),
            serde_json::from_value(
                json!({"capabilities":[],"boot_id":"boot","session_id":"session","observed_ms":1}),
            )
            .unwrap(),
        );
    }
    support::enroll(&mut state, "node-000");
    state
        .node_groups
        .get_mut("hosts")
        .unwrap()
        .node_uids
        .insert("node-000".into());
    let plan:converyn_core::model::domain::PlanInput=serde_json::from_value(json!({"project":"project-0","owner_uid":"example","batch_size":1,"playbook":{"api_version":"converyn/v1","name":"fixture","steps":[{"id":"health","phase":"rollout","scope":"per_node","operation":{"kind":"http_check","url":"https://example.invalid/","expected_status":200,"timeout_seconds":1},"hosts":"worker"}]},"inventory":{"nodes":[{"uid":"node-000","hostname":"node-000","address":"192.0.2.1"}],"groups":{"worker":["node-000"]}},"hosts":{"node_group_uid":"hosts","revision":1,"node_uids":["node-000"]}})).unwrap();
    for i in 0..20 {
        let source=serde_json::from_value::<converyn_core::orchestration::workflow::Revision>(json!({"uid":format!("flow-{i}"),"name":format!("Workflow {i}"),"targets":{"qa":{"node_group_uid":"hosts","hosts":"all"}},"playbook":plan.playbook,"inventory":plan.inventory})).unwrap();
        assert!(
            state
                .apply(Command::Workflow {
                    change: converyn_core::orchestration::workflow::Change::Register {
                        workflow: Box::new(source),
                        expected_revision: 0
                    }
                })
                .ok
        );
    }
    state.apply(Command::CreateRun {
        output_key: None,
        run_id: "template".into(),
        idempotency_key: "template".into(),
        plan: Box::new(plan.compile().unwrap()),
    });
    state.apply(Command::Cancel {
        run_id: "template".into(),
    });
    let template = state.runs["template"].clone();
    state
        .history_change(Change::Archive {
            kind: Kind::Run,
            uid: "template".into(),
            expected_revision: template.revision,
            now_ms: 1,
        })
        .unwrap();
    let record = state.history.records["run/template"].as_ref().clone();
    state.history.records.clear();
    state.idempotency.clear();
    for i in 0..10000 {
        let mut run = template.as_ref().clone();
        run.id = format!("archive-{i:05}");
        run.sequence = i + 1;
        let bytes = serde_json::to_vec(&run).unwrap();
        let mut r = record.clone();
        r.uid = run.id.clone();
        r.sequence = run.sequence;
        r.sha256 = hex::encode(Sha256::digest(&bytes));
        r.bytes = bytes.len() as u64;
        tx.execute(
            "INSERT INTO cold_history(kind,uid,sha256,data) VALUES('run',?1,?2,?3)",
            rusqlite::params![r.uid, r.sha256, bytes],
        )
        .unwrap();
        state
            .history
            .records
            .insert(format!("run/{}", r.uid), std::sync::Arc::new(r));
    }
    state.registry.audit = (0..100000)
        .map(|i| json!({"action":"capacity_fixture","index":i,"principal":"unknown"}))
        .collect();
    audit_store::compact(&tx, &mut state).unwrap();
    state_rows::save(&tx, &crate::support::state(), &state).unwrap();
    tx.commit().unwrap();
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}
fn req(c: &reqwest::Client, base: &str, path: &str) -> reqwest::RequestBuilder {
    c.get(format!("{base}{path}"))
        .bearer_auth("capacity-token-abcdefghijklmnopqrstuvwxyz")
        .header("x-converyn-cluster", "capacity")
}
fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_hundred_nodes_twenty_operators_and_large_history_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    drop(listener);
    let server = Server(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture", "--nocapture"])
            .env("CONVERYN_CAPACITY_DIR", tmp.path())
            .env("CONVERYN_CAPACITY_ADDR", &addr)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let base = format!("http://{addr}");
    let c = client();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if c.get(format!("{base}/health/live")).send().await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let response = req(&c, &base, "/api/v1/controlplane/init").build().unwrap();
    let mut r = c
        .post(response.url().clone())
        .headers(response.headers().clone())
        .json(&json!({"1":{"addr":addr}}));
    let _ = &mut r;
    assert!(r.send().await.unwrap().status().is_success());
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if req(&c, &base, "/health/ready")
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
    let mut tasks = Vec::new();
    let began = Instant::now();
    for operator in 0..20 {
        let base = base.clone();
        tasks.push(tokio::spawn(async move {
            let c = client();
            let mut times = Vec::new();
            for i in 0..25 {
                let path = if (i + operator) % 2 == 0 {
                    "/api/v1/runs?limit=50"
                } else {
                    "/api/v1/workflows"
                };
                let start = Instant::now();
                let r = req(&c, &base, path).send().await.unwrap();
                assert!(r.status().is_success());
                let bytes = r.bytes().await.unwrap();
                assert!(bytes.len() < 256 * 1024);
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
            times
        }));
    }
    let write_base = base.clone();
    let writes = tokio::spawn(async move {
        let c = client();
        let mut times = Vec::new();
        for i in 0..300 {
            let start = Instant::now();
            let r=c.post(format!("{write_base}/api/v1/prototype/commands")).bearer_auth("capacity-token-abcdefghijklmnopqrstuvwxyz").header("x-converyn-cluster","capacity").json(&json!({"type":"observe_node","node_uid":format!("node-{i:03}"),"hardware_sha256":converyn_core::model::domain::digest(&format!("node-{i:03}")),"boot_id":"boot","session_id":"session","observed_ms":2})).send().await.unwrap();
            assert_eq!(r.status(), 200);
            let v: Value = r.json().await.unwrap();
            assert_eq!(v["ok"], true);
            times.push(start.elapsed().as_secs_f64() * 1000.);
        }
        times
    });
    let mut query = Vec::new();
    for task in tasks {
        query.extend(task.await.unwrap())
    }
    let mut writes = writes.await.unwrap();
    let query_p95 = percentile(&mut query, 0.95);
    let query_p99 = percentile(&mut query, 0.99);
    let write_p95 = percentile(&mut writes, 0.95);
    let rss = std::fs::read_to_string(format!("/proc/{}/status", server.0.id()))
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .unwrap_or("unavailable")
        .to_owned();
    let report = json!({"passed":query_p95<=500.&&query_p99<=1500.&&write_p95<=1000.,"topology":"single controller, local loopback HTTP development protocol; mTLS authorization tested separately","simulated_nodes":300,"workflows":20,"concurrent_operators":20,"historical_runs":10000,"historical_audit_records":100000,"query_requests":query.len(),"heartbeat_writes":writes.len(),"query_p95_ms":query_p95,"query_p99_ms":query_p99,"write_p95_ms":write_p95,"elapsed_seconds":began.elapsed().as_secs_f64(),"available_cpus":std::thread::available_parallelism().unwrap().get(),"controller_rss":rss});
    if let Ok(path) = std::env::var("CONVERYN_CAPACITY_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!("{report}");
    assert_eq!(report["passed"], true);
}
