//! Sequential autonomous execution. An active intent is durable before Claim;
//! its exact grant, boot/session and report sequence survive retries/restarts.
use crate::credentials::security::Tls;
use crate::executors::runner::{Outcome, Receipt};
use anyhow::{Context, Result, ensure};
use converyn_core::model::domain::{Operation, Work};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "crate::executors::log_retention::default_limit")]
    pub log_max_bytes: u64,
    pub node_uid: String,
    pub cluster: String,
    pub servers: Vec<String>,
    pub token_file: PathBuf,
    pub tls: Tls,
    pub state_dir: PathBuf,
    #[serde(default)]
    pub artifact_sources: crate::artifacts::source::Sources,
    #[serde(default)]
    pub execution: Option<crate::executors::execution::Policy>,
    #[serde(default)]
    pub hardware_id_file: Option<PathBuf>,
    #[serde(default)]
    pub enrolled: bool,
}
fn terminal_sequence() -> u64 {
    2
}

/// Authentication failures are permanent for this request, unlike transport loss.
#[derive(Debug)]
pub struct AuthenticationRejected {
    pub status: u16,
}
impl std::fmt::Display for AuthenticationRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.status == 403 {
            write!(
                f,
                "Permission denied (HTTP 403). Check workflow and preset grants with `converyn auth status` or `converyn auth can-i`; contact an access administrator if needed."
            )
        } else {
            write!(
                f,
                "Personal credential rejected (HTTP {}). Check your selected --context and certificate expiry; run `converyn auth doctor` with the intended context.",
                self.status
            )
        }
    }
}
impl std::error::Error for AuthenticationRejected {}

#[derive(Clone, Deserialize, Serialize)]
struct Active {
    #[serde(default)]
    output_key: Option<converyn_core::model::outputs::PublicKey>,
    #[serde(default)]
    run_sequence: u64,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    owner_uid: Option<String>,
    #[serde(default = "terminal_sequence")]
    terminal_sequence: u64,
    run_id: String,
    work: Work,
    grant: String,
    boot_id: String,
    session_id: String,
    #[serde(default)]
    receipt: Option<Receipt>,
}

pub fn durable(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("missing parent")?;
    let temporary = path.with_extension("tmp");
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(temporary, path)?;
    let mut ancestor = Some(parent);
    while let Some(p) = ancestor {
        File::open(p)?.sync_all()?;
        ancestor = p.parent();
    }
    Ok(())
}
pub(super) fn random_id() -> Result<String> {
    let mut b = [0; 24];
    File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(hex::encode(b))
}

#[derive(Clone)]
pub struct Client {
    pub preferred: Arc<std::sync::atomic::AtomicUsize>,
    pub fingerprint: Arc<std::sync::RwLock<Option<String>>>,
    pub hardware: Option<String>,
    pub session: Option<String>,
    pub http: Arc<std::sync::RwLock<reqwest::Client>>,
    pub servers: Vec<String>,
    pub token: String,
    pub cluster: String,
}
impl Client {
    /// Retry only configured endpoints. A follower's arbitrary hint cannot redirect credentials.
    pub async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value)> {
        let first = self.preferred.load(Ordering::Relaxed);
        for offset in 0..self.servers.len() {
            let index = (first + offset) % self.servers.len();
            let server = &self.servers[index];
            let http = self
                .http
                .read()
                .map_err(|_| anyhow::anyhow!("HTTP client lock poisoned"))?
                .clone();
            let mut r = http
                .request(
                    method.clone(),
                    format!("{}{path}", server.trim_end_matches('/')),
                )
                .bearer_auth(&self.token)
                .header("x-converyn-cluster", &self.cluster)
                .header(
                    "x-converyn-protocol",
                    crate::controller::network::PROTOCOL_VERSION,
                );
            if let Some(hardware) = &self.hardware {
                r = r.header("x-converyn-hardware", hardware);
            }
            if let Some(session) = &self.session {
                r = r.header("x-converyn-session", session);
            }
            if let Some(body) = body {
                r = r.json(body);
            }
            let Ok(mut response) = r.send().await else {
                continue;
            };
            let status = response.status().as_u16();
            if status == 429 || status >= 500 {
                // Drain small error responses so follower requests return their
                // connection to the pool instead of causing a TLS reconnect storm.
                let mut drained = 0usize;
                while let Ok(Some(chunk)) = response.chunk().await {
                    drained += chunk.len();
                    if drained > 65536 {
                        break;
                    }
                }
                continue;
            }
            let value = response.json().await.unwrap_or(Value::Null);
            if status == 401 || status == 403 {
                return Err(AuthenticationRejected { status }.into());
            }
            self.preferred.store(index, Ordering::Relaxed);
            return Ok((status, value));
        }
        anyhow::bail!("no configured controller can confirm the request")
    }
}
struct Agent {
    telemetry: Arc<std::sync::RwLock<super::inventory::Cache>>,
    inventory_sent: std::sync::Mutex<Option<u64>>,
    cfg: Config,
    config_path: PathBuf,
    api: Client,
    boot: String,
    session: String,
}
impl Agent {
    async fn fetch_artifact(
        &self,
        artifact: &crate::artifacts::source::ArtifactRef,
        run: &str,
        work: &str,
        cache: &Path,
    ) -> Result<crate::artifacts::artifact::Download> {
        if artifact.locations.is_empty() {
            let path = self.path(&format!(
                "runs/{run}/work/{work}/artifacts/{}",
                artifact.sha256
            ));
            crate::artifacts::transfer::download(
                &self.api,
                artifact,
                cache,
                &path,
                self.cfg.artifact_sources.cache_max_bytes,
                self.cfg.artifact_sources.bytes_per_second,
            )
            .await
        } else {
            self.cfg.artifact_sources.fetch(artifact, cache).await
        }
    }
    fn local_status(
        &self,
        confirmed: bool,
        diagnostic: Option<String>,
        next_seconds: u64,
    ) -> Result<()> {
        let active = fs::read(self.cfg.state_dir.join("active.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Active>(&bytes).ok());
        let phase =
            if let Some(job) = &active {
                if job.receipt.as_ref().is_some_and(|r| {
                    matches!(r.outcome, Outcome::NeedsAttention | Outcome::TimedOut)
                }) {
                    "needs_attention"
                } else if job.receipt.is_some() {
                    "outbox_pending"
                } else {
                    "authorized_or_executing"
                }
            } else if confirmed {
                "ready"
            } else {
                "controller_or_identity_unavailable"
            };
        durable(
            &self.cfg.state_dir.join("status.json"),
            &serde_json::to_vec(
                &json!({"node_uid":self.cfg.node_uid,"cluster":self.cfg.cluster,"pid":std::process::id(),"boot_id":self.boot,"session_id":self.session,"hardware_sha256":self.api.hardware,"controller_confirmed":confirmed,"phase":phase,"diagnostic":diagnostic,"observed_ms":crate::executors::host::now_ms(),"next_check_ms":crate::executors::host::now_ms()+next_seconds*1000,"run_id":active.as_ref().map(|j|&j.run_id)}),
            )?,
        )
    }
    async fn heartbeat(&self) -> Result<()> {
        let cache = self.telemetry.read().map(|v| v.clone()).unwrap_or_default();
        let sent = self.inventory_sent.lock().ok().and_then(|v| *v);
        let inventory = cache.inventory.filter(|v| Some(v.collected_ms) != sent);
        let mut body = json!({"boot_id":self.boot,"session_id":self.session,"hardware_sha256":self.api.hardware,"capabilities":crate::plugins::registry().descriptors().into_iter().filter(|d|d.location==converyn_plugin_api::Location::Node).map(|d|d.identity).collect::<Vec<_>>()});
        if let Ok(bytes) = fs::read(self.cfg.state_dir.join("status.json"))
            && let Ok(status) = serde_json::from_slice::<Value>(&bytes)
            && let Some(diagnostic) = status["diagnostic"].as_str()
        {
            body["diagnostic"] = json!(diagnostic.chars().take(512).collect::<String>());
        }
        if let Some(v) = &inventory {
            body["inventory"] = serde_json::to_value(v)?;
        }
        if let Some(v) = &cache.resources {
            body["resources"] = serde_json::to_value(v)?;
        }
        if let Some(v) = cache.checked_ms {
            body["inventory_checked_ms"] = json!(v);
        }

        let (s, response) = self
            .api
            .request(reqwest::Method::POST, &self.path("heartbeat"), Some(&body))
            .await?;
        ensure!(
            s == 200,
            "heartbeat unconfirmed: {}",
            response["code"]
                .as_str()
                .unwrap_or("controller unavailable")
        );
        if let Some(v) = inventory
            && let Ok(mut sent) = self.inventory_sent.lock()
        {
            *sent = Some(v.collected_ms);
        }
        if let Ok((200, logging)) = self
            .api
            .request(reqwest::Method::GET, &self.path("log-storage"), None)
            .await
            && let Err(error) = durable(
                &self.cfg.state_dir.join("log-storage.json"),
                &serde_json::to_vec(&logging)?,
            )
        {
            tracing::warn!(%error,"cannot persist optional log configuration");
        }
        self.local_status(true, None, 20)?;
        Ok(())
    }
    fn path(&self, tail: &str) -> String {
        format!("/api/v1/agents/{}/{}", self.cfg.node_uid, tail)
    }
    async fn command(&self, body: Value) -> Result<(u16, Value)> {
        self.api
            .request(reqwest::Method::POST, &self.path("commands"), Some(&body))
            .await
    }
    async fn attempt(&self, job: &Active) -> Result<Value> {
        let (s, v) = self
            .api
            .request(
                reqwest::Method::GET,
                &self.path(&format!("attempts/{}/{}", job.run_id, job.work.id)),
                None,
            )
            .await?;
        ensure!(s == 200, "assigned attempt unavailable");
        let assigned: Work = serde_json::from_value(v["work"].clone())
            .context("authoritative work input missing")?;
        ensure!(
            assigned == job.work
                && (job.owner_uid.is_none()
                    || (v["project"].as_str() == job.project.as_deref()
                        && v["owner_uid"].as_str() == job.owner_uid.as_deref())),
            "local intent differs from committed work; execution stopped"
        );
        Ok(v["attempt"].clone())
    }
    async fn workflow_environment(
        &self,
        job: &Active,
    ) -> Result<std::collections::BTreeMap<String, String>> {
        let (status, value) = self
            .api
            .request(
                reqwest::Method::GET,
                &self.path(&format!("attempts/{}/{}", job.run_id, job.work.id)),
                None,
            )
            .await?;
        ensure!(
            status == 200 && serde_json::from_value::<Work>(value["work"].clone())? == job.work,
            "workflow context differs from committed work"
        );
        let mut environment = std::collections::BTreeMap::new();
        let files: std::collections::BTreeMap<
            String,
            converyn_core::orchestration::inventory::PackageFile,
        > = serde_json::from_value(value["bundle"].clone())?;
        if !files.is_empty() {
            let policy = self
                .cfg
                .execution
                .as_ref()
                .context("workflow package requires execution policy")?;
            let location = super::workflow_package::location(&policy.work_root, &files);
            if !location.exists() {
                let mut contents = std::collections::BTreeMap::new();
                let mut heartbeat = tokio::time::Instant::now();
                for file in files.values() {
                    if contents.contains_key(&file.sha256) {
                        continue;
                    }
                    let (status, data) = self
                        .api
                        .request(
                            reqwest::Method::GET,
                            &self.path(&format!("scripts/{}", file.sha256)),
                            None,
                        )
                        .await?;
                    ensure!(status == 200, "workflow package definition unavailable");
                    contents.insert(
                        file.sha256.clone(),
                        data["content"]
                            .as_str()
                            .context("invalid package definition")?
                            .to_owned(),
                    );
                    if heartbeat.elapsed() >= Duration::from_secs(20) {
                        let _ = self.heartbeat().await;
                        heartbeat = tokio::time::Instant::now();
                    }
                }
                super::workflow_package::install(&policy.work_root, &files, &contents)?;
            }
            super::workflow_package::verify(&location, &files)?;
            environment.insert(
                "CONVERYN_PACKAGE_ROOT".into(),
                location.to_string_lossy().into_owned(),
            );
        }
        if !value["inventory"].is_null() {
            let inventory: converyn_core::orchestration::inventory::Inventory =
                serde_json::from_value(value["inventory"].clone())?;
            environment.insert(
                "CONVERYN_INVENTORY_JSON".into(),
                inventory.context(&self.cfg.node_uid)?,
            );
        }
        Ok(environment)
    }
    fn save(&self, job: &Active) -> Result<()> {
        durable(
            &self.cfg.state_dir.join("active.json"),
            &serde_json::to_vec(job)?,
        )
    }
    fn output_context(&self, job: &Active) -> crate::executors::outputs::Context {
        crate::executors::outputs::Context {
            run_id: job.run_id.clone(),
            work_id: job.work.id.clone(),
            specifications: job.work.outputs.clone(),
            key: job.output_key.clone(),
        }
    }
    fn finish_native_outputs(&self, job: &Active, receipt: Receipt) -> Result<Receipt> {
        crate::executors::outputs::native_receipt(
            &self.cfg.state_dir.join("native-outputs"),
            &self.output_context(job),
            receipt,
        )
    }
    fn clear(&self) -> Result<()> {
        if let Ok(bytes) = fs::read(self.cfg.state_dir.join("active.json"))
            && let Ok(job) = serde_json::from_slice::<Active>(&bytes)
        {
            if let Some(receipt) = &job.receipt
                && receipt.outcome == Outcome::NeedsAttention
            {
                crate::executors::log_retention::mark_resolved(
                    &self.cfg.state_dir,
                    &format!("{}/{}", job.run_id, job.work.id),
                    !matches!(job.work.operation, Operation::Exec { .. }),
                    receipt,
                )?;
            }
            use sha2::{Digest, Sha256};
            let key = hex::encode(Sha256::digest(format!("{}/{}", job.run_id, job.work.id)));
            let path = self
                .cfg
                .state_dir
                .join("executions")
                .join(key)
                .join("environment.json");
            crate::executors::outputs::quarantine(path.parent().unwrap())?;
            if path.exists() {
                fs::remove_file(&path)?;
                File::open(path.parent().unwrap())?.sync_all()?;
            }
        }
        fs::remove_file(self.cfg.state_dir.join("active.json"))?;
        File::open(&self.cfg.state_dir)?.sync_all()?;
        Ok(())
    }
    async fn report(&self, j: &Active, sequence: u64, status: &str) -> Result<()> {
        let logging = fs::read(self.cfg.state_dir.join("log-storage.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .is_some_and(|v| v["enabled"] == true);
        if logging && let Some(logs) = j.receipt.as_ref().and_then(|r| r.logs.as_ref()) {
            let queue_result = (|| -> Result<()> {
                use sha2::{Digest, Sha256};
                let queue = self.cfg.state_dir.join("uploads");
                fs::create_dir_all(&queue)?;
                let key = hex::encode(Sha256::digest(format!("{}/{}", j.run_id, j.work.id)));
                let pending = queue.join(format!("{key}.json"));
                ensure!(
                    pending.exists()
                        || fs::read_dir(&queue)?
                            .filter_map(Result::ok)
                            .filter(|e| e.file_name().to_string_lossy().ends_with(".json")
                                && !e.file_name().to_string_lossy().ends_with(".cursor.json"))
                            .take(4096)
                            .count()
                            < 4096,
                    "log upload queue full; raw logs remain subject to local retention"
                );
                let job = json!({"run_id":j.run_id,"work":{"id":j.work.id,"operation":{"kind":if matches!(j.work.operation,Operation::Exec{..}){"exec"}else{"native"}}},"receipt":{"logs":{"stdout":{"stored_bytes":logs.stdout.stored_bytes},"stderr":{"stored_bytes":logs.stderr.stored_bytes}}}});
                durable(&pending, &serde_json::to_vec(&job)?)?;
                Ok(())
            })();
            if let Err(error) = queue_result {
                tracing::warn!(%error,run_id=%j.run_id,work_id=%j.work.id,"optional log upload queue unavailable; original execution receipt retained");
            }
        }
        let (s,v)=self.command(json!({"type":"report","run_id":j.run_id,"work_id":j.work.id,"node_uid":self.cfg.node_uid,"grant":j.grant,"sequence":sequence,"status":status,"receipt":j.receipt})).await?;
        ensure!(
            s == 200 && v["ok"] == true,
            "report rejected; durable outbox retained"
        );
        Ok(())
    }
    async fn step(&self) -> Result<()> {
        let active = self.cfg.state_dir.join("active.json");
        if !active.exists() {
            let (s, poll) = self
                .api
                .request(reqwest::Method::GET, &self.path("work"), None)
                .await?;
            ensure!(s == 200, "work query rejected");
            let items = poll["work"].as_array().context("invalid work response")?;
            let pin_result = if let Some(pins) = poll.get("artifact_pins") {
                let pins = serde_json::from_value(pins.clone())?;
                crate::artifacts::cache::set_pins(
                    &self.cfg.state_dir.join("artifacts"),
                    &pins,
                    self.cfg.artifact_sources.cache_max_bytes,
                )
            } else {
                Ok(())
            };
            if let Some(item) = items.iter().find(|item| item["prefetch"] == true) {
                let work: Work = serde_json::from_value(item["work"].clone())?;
                ensure!(
                    work.node_uid == self.cfg.node_uid,
                    "prefetch assigned to another node"
                );
                let artifact = work
                    .operation
                    .artifact()
                    .context("prefetch artifact missing")?;
                let result = match pin_result {
                    Err(error) => Err(error),
                    Ok(()) => {
                        let cache = self.cfg.state_dir.join("artifacts");
                        let fetch = self.fetch_artifact(
                            artifact,
                            item["run_id"].as_str().context("run identity missing")?,
                            &work.id,
                            &cache,
                        );
                        tokio::pin!(fetch);
                        loop {
                            tokio::select! {
                                result = &mut fetch => break result,
                                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                                    let _ = self.heartbeat().await;
                                    let path = self.path(&format!("attempts/{}/{}", item["run_id"].as_str().context("run identity missing")?, work.id));
                                    if let Ok((200, value)) = self.api.request(reqwest::Method::GET, &path, None).await
                                        && matches!(value["run_status"].as_str(), Some("canceled" | "failed" | "paused")) {
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                };
                let mut change = json!({"action":"prepared","run_id":item["run_id"],"work_id":work.id,"node_uid":self.cfg.node_uid,"sha256":artifact.sha256,"size":artifact.size});
                if let Err(error) = &result {
                    change = json!({"action":"failed","run_id":item["run_id"],"work_id":work.id,"node_uid":self.cfg.node_uid,"reason":error.to_string().chars().take(2048).collect::<String>()});
                }
                let (status, reply) = self
                    .api
                    .request(
                        reqwest::Method::POST,
                        &self.path("commands"),
                        Some(&json!({"type":"preparation","change":change})),
                    )
                    .await?;
                ensure!(
                    status == 200 && reply["ok"] == true,
                    "preparation report deferred"
                );
                result?;
                return Ok(());
            }
            pin_result?;
            let Some((item, work)) = items.iter().find_map(|item| {
                if item["claimable"] != true {
                    return None;
                }
                let work: Work = serde_json::from_value(item["work"].clone()).ok()?;
                Some((item, work))
            }) else {
                return Ok(());
            };
            validate_native_operation(
                &self.cfg,
                &self.config_path,
                &std::env::current_exe()?,
                &work.operation,
                item["project"].as_str(),
            )?;
            if let Operation::Plugin { call } = &work.operation {
                crate::plugins::authorize_node(
                    &crate::executors::native::managed_policy(
                        &work.operation,
                        item["project"].as_str(),
                    )?,
                    call,
                    item["project"].as_str(),
                )?;
            }
            crate::executors::authorization::policy(
                &self.cfg,
                &work.operation,
                item["project"].as_str(),
            )?;
            ensure!(
                work.node_uid == self.cfg.node_uid,
                "work assigned to another node"
            );
            if matches!(
                work.operation,
                Operation::Exec { .. } | Operation::Package { .. }
            ) {
                let journal = self.cfg.state_dir.clone();
                let available = self.cfg.log_max_bytes - 32 * 1024 * 1024;
                tokio::task::spawn_blocking(move || {
                    crate::executors::log_retention::collect_agent(&journal, available)
                })
                .await??;
            }
            self.save(&Active {
                output_key: serde_json::from_value(item["output_key"].clone())?,
                run_sequence: item["run_sequence"]
                    .as_u64()
                    .context("run sequence missing")?,
                project: item["project"].as_str().map(String::from),
                owner_uid: item["owner_uid"].as_str().map(String::from),
                terminal_sequence: 2,
                run_id: item["run_id"].as_str().context("run id missing")?.into(),
                work,
                grant: random_id()?,
                boot_id: self.boot.clone(),
                session_id: self.session.clone(),
                receipt: None,
            })?;
        }
        let mut job: Active = serde_json::from_slice(&fs::read(&active)?)?;
        ensure!(
            job.work.node_uid == self.cfg.node_uid,
            "local intent belongs to another node"
        );
        let (status,reply)=self.command(json!({"type":"claim","input_sha256":job.work.input_sha256,"run_id":job.run_id,"work_id":job.work.id,"node_uid":self.cfg.node_uid,"grant":job.grant,"boot_id":job.boot_id,"session_id":job.session_id})).await?;
        if status == 409 && reply["code"] == "stale_agent_session" {
            let attempt = self.attempt(&job).await?;
            if attempt["grant"].is_null() && attempt["status"] == "pending" {
                job.boot_id = self.boot.clone();
                job.session_id = self.session.clone();
                job.grant = random_id()?;
                self.save(&job)?;
                return Ok(());
            }
        }
        if status == 409 && reply["code"] == "not_claimable" {
            let a = self.attempt(&job).await?;
            // No matching grant was committed: no execution was authorized locally.
            if a["grant"].is_null() && a["status"] == "canceled" {
                self.clear()?;
                return Ok(());
            }
        }
        ensure!(
            status == 200 && reply["ok"] == true,
            "claim unavailable ({}); intent retained",
            reply["code"].as_str().unwrap_or("transport_unavailable")
        );
        let attempt = self.attempt(&job).await?;
        if matches!(
            attempt["status"].as_str(),
            Some("succeeded" | "failed" | "canceled")
        ) {
            self.clear()?;
            return Ok(());
        }
        if job.receipt.is_none()
            && matches!(
                job.work.operation,
                Operation::Exec { .. }
                    | Operation::Package { .. }
                    | Operation::Plugin { .. }
                    | Operation::Reboot { .. }
            )
        {
            use sha2::{Digest, Sha256};
            let id = format!("{}/{}", job.run_id, job.work.id);
            let key = hex::encode(Sha256::digest(&id));
            let journal = if matches!(job.work.operation, Operation::Exec { .. }) {
                self.cfg
                    .state_dir
                    .join("executions")
                    .join(&key)
                    .join("receipt.json")
            } else {
                self.cfg
                    .state_dir
                    .join("native")
                    .join(format!("{key}.json"))
            };
            let resumed = job.session_id != self.session
                && attempt["last_sequence"].as_u64().unwrap_or(0) > 0;
            let boot_changed =
                matches!(job.work.operation, Operation::Exec { .. }) && job.boot_id != self.boot;
            if !journal.exists() && (resumed || boot_changed) {
                let (digest, timeout, native) = match &job.work.operation {
                    Operation::Exec {
                        script_sha256,
                        timeout_seconds,
                        ..
                    } => (script_sha256.clone(), u64::from(*timeout_seconds), None),
                    operation => (
                        String::new(),
                        0,
                        Some(Box::new(crate::executors::native::Evidence::empty(
                            operation,
                        ))),
                    ),
                };
                job.receipt=Some(Receipt{ outputs: Default::default(), diagnostic:Some("local execution evidence missing after restart; side effects are not replayed".into()),execution:None,logs:None,native,operation_id:id,script_sha256:digest,timeout_seconds:timeout,outcome:Outcome::NeedsAttention,exit_code:None,boot_id:job.boot_id.clone()});
                if attempt["status"] == "needs_attention" && !attempt["receipt"].is_null() {
                    job.receipt = Some(serde_json::from_value(attempt["receipt"].clone())?);
                    job.terminal_sequence = attempt["last_sequence"]
                        .as_u64()
                        .context("report sequence missing")?;
                }
                self.save(&job)?;
            }
        }
        if let Some(receipt) = &job.receipt {
            let status = match receipt.outcome {
                Outcome::Succeeded => "succeeded",
                Outcome::Failed => "failed",
                Outcome::Canceled => "canceled",
                Outcome::TimedOut | Outcome::NeedsAttention | Outcome::Running => "needs_attention",
            };
            self.report(&job, job.terminal_sequence, status).await?;
            if status != "needs_attention" {
                self.clear()?;
            }
            return Ok(());
        }
        if !matches!(&job.work.operation, Operation::Exec { .. }) {
            if let Some(receipt) = crate::executors::native::cached_receipt(
                &self.cfg.state_dir.join("native"),
                &format!("{}/{}", job.run_id, job.work.id),
                &job.work.operation,
            )? {
                let receipt = if receipt.outcome == Outcome::Running {
                    crate::executors::native::execute(
                        &self.cfg.state_dir.join("native"),
                        &format!("{}/{}", job.run_id, job.work.id),
                        &crate::executors::native::managed_policy(
                            &job.work.operation,
                            job.project.as_deref(),
                        )?,
                        &job.work.operation,
                        &[],
                        &AtomicBool::new(false),
                        None,
                    )?
                } else {
                    receipt
                };
                if receipt.outcome != Outcome::Running {
                    job.receipt = Some(self.finish_native_outputs(&job, receipt)?);
                    self.save(&job)?;
                }
                return Ok(());
            }
            if attempt["status"] == "cancel_requested" {
                let seq = attempt["last_sequence"]
                    .as_u64()
                    .context("sequence missing")?
                    + 1;
                self.report(&job, seq, "canceled").await?;
                self.clear()?;
                return Ok(());
            }
            self.report(&job, 1, "running").await?;
            let content = if let Some(sha256) = job.work.operation.definition_sha() {
                let (s, data) = self
                    .api
                    .request(
                        reqwest::Method::GET,
                        &self.path(&format!("scripts/{sha256}")),
                        None,
                    )
                    .await?;
                ensure!(s == 200, "file definition unavailable");
                data["content"]
                    .as_str()
                    .context("invalid definition")?
                    .as_bytes()
                    .to_vec()
            } else {
                Vec::new()
            };
            let artifact_path = if let Some(artifact) = job.work.operation.artifact() {
                let cache = self.cfg.state_dir.join("artifacts");
                let fetch = self.fetch_artifact(artifact, &job.run_id, &job.work.id, &cache);
                tokio::pin!(fetch);
                let mut last_heartbeat = tokio::time::Instant::now();
                let download = loop {
                    tokio::select! {
                        r=&mut fetch=>break r?,
                        _=tokio::time::sleep(Duration::from_secs(1))=>{
                            if last_heartbeat.elapsed()>=Duration::from_secs(20){let _=self.heartbeat().await;last_heartbeat=tokio::time::Instant::now();}
                            if let Ok(a)=self.attempt(&job).await && a["status"]=="cancel_requested" {
                                self.report(&job,2,"canceled").await?;self.clear()?;return Ok(());
                            }
                        }
                    }
                };
                Some(download.path)
            } else {
                None
            };
            let mut policy = crate::executors::native::managed_policy(
                &job.work.operation,
                job.project.as_deref(),
            )?;
            let variables = crate::credentials::secrets::environment(&job.work.operation)?;
            if variables
                .values()
                .any(|v| !matches!(v, converyn_core::model::variables::Value::Literal(_)))
            {
                let path = self.path(&format!(
                    "runs/{}/work/{}/inputs?grant={}",
                    job.run_id, job.work.id, job.grant
                ));
                let (status, body) = self.api.request(reqwest::Method::GET, &path, None).await?;
                ensure!(status == 200, "required task Secret input unavailable");
                policy.runtime_inputs = Some(serde_json::from_value(body)?);
            }
            policy.runtime_fence = job.run_sequence;
            policy.runtime_project = job.project.clone();
            policy.runtime_owner = job.owner_uid.clone();
            policy.runtime_state_dir = Some(self.cfg.state_dir.clone());

            let operation = job.work.operation.clone();
            let state = self.cfg.state_dir.join("native");
            let id = format!("{}/{}", job.run_id, job.work.id);
            let cancel = Arc::new(AtomicBool::new(false));
            let stop = cancel.clone();
            let mut child = tokio::task::spawn_blocking(move || {
                crate::executors::native::execute(
                    &state,
                    &id,
                    &policy,
                    &operation,
                    &content,
                    &stop,
                    artifact_path.as_deref(),
                )
            });
            let mut last_heartbeat = tokio::time::Instant::now();
            let receipt = loop {
                tokio::select! {
                    r=&mut child=>break r??,
                    _=tokio::time::sleep(Duration::from_secs(1))=> {
                        if last_heartbeat.elapsed()>=Duration::from_secs(20) {let _=self.heartbeat().await;last_heartbeat=tokio::time::Instant::now();}
                        if let Ok(a)=self.attempt(&job).await && a["status"]=="cancel_requested" {cancel.store(true,Ordering::SeqCst);}
                    }
                }
            };
            if receipt.outcome != Outcome::Running {
                job.receipt = Some(self.finish_native_outputs(&job, receipt)?);
                self.save(&job)?;
            }
            return Ok(());
        }
        let (digest, timeout) = match &job.work.operation {
            Operation::Exec {
                script_sha256,
                timeout_seconds,
                ..
            } => (script_sha256.clone(), *timeout_seconds),
            _ => anyhow::bail!("unsupported operation"),
        };
        let script = self.cfg.state_dir.join(format!("{digest}.sh"));
        let operation_id = format!("{}/{}", job.run_id, job.work.id);
        let journal = self.cfg.state_dir.join("executions");
        use sha2::{Digest, Sha256};
        let has_receipt = journal
            .join(hex::encode(Sha256::digest(operation_id.as_bytes())))
            .join("receipt.json")
            .exists();
        if !has_receipt {
            if attempt["status"] == "cancel_requested" {
                let seq = attempt["last_sequence"]
                    .as_u64()
                    .context("sequence missing")?
                    + 1;
                self.report(&job, seq, "canceled").await?;
                self.clear()?;
                return Ok(());
            }
            let (s, data) = self
                .api
                .request(
                    reqwest::Method::GET,
                    &self.path(&format!("scripts/{digest}")),
                    None,
                )
                .await?;
            ensure!(s == 200, "script definition unavailable");
            let content = data["content"]
                .as_str()
                .context("invalid script response")?;
            ensure!(
                content.len() <= 65536 && hex::encode(Sha256::digest(content.as_bytes())) == digest,
                "script digest mismatch"
            );
            durable(&script, content.as_bytes())?;
        }
        let mut environment = if has_receipt {
            crate::credentials::secrets::Resolved::default()
        } else if let Operation::Exec { environment, .. } = &job.work.operation {
            let local = environment
                .iter()
                .filter(|(_, v)| matches!(v, converyn_core::model::variables::Value::Literal(_)))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            crate::credentials::secrets::resolve_literals(&local)?
        } else {
            unreachable!()
        };
        if !has_receipt
            && matches!(&job.work.operation,Operation::Exec{environment,..} if environment.values().any(|v|matches!(v,converyn_core::model::variables::Value::OutputSecret(_) | converyn_core::model::variables::Value::Secret(_))))
        {
            let path = self.path(&format!(
                "runs/{}/work/{}/inputs?grant={}",
                job.run_id, job.work.id, job.grant
            ));
            let (status, body) = self.api.request(reqwest::Method::GET, &path, None).await?;
            if status == 410 || status == 422 {
                job.receipt = Some(Receipt {
                    outputs: Default::default(),
                    diagnostic: Some("required_run_secret_expired_or_invalid".into()),
                    execution: None,
                    logs: None,
                    native: None,
                    operation_id: format!("{}/{}", job.run_id, job.work.id),
                    script_sha256: digest.clone(),
                    timeout_seconds: u64::from(timeout),
                    outcome: Outcome::Failed,
                    exit_code: None,
                    boot_id: job.boot_id.clone(),
                });
                self.save(&job)?;
                return Ok(());
            }
            ensure!(
                status == 200,
                "Run secret input unavailable; restore the output keyring before resuming"
            );
            let inputs: crate::credentials::secrets::Resolved = serde_json::from_value(body)?;
            let Operation::Exec {
                environment: declared,
                ..
            } = &job.work.operation
            else {
                unreachable!()
            };
            let expected: std::collections::BTreeSet<_> = declared
                .iter()
                .filter(|(_, v)| {
                    matches!(
                        v,
                        converyn_core::model::variables::Value::OutputSecret(_)
                            | converyn_core::model::variables::Value::Secret(_)
                    )
                })
                .map(|(k, _)| k.clone())
                .collect();
            ensure!(
                expected == inputs.values.keys().cloned().collect(),
                "Run secret inputs differ from frozen intent"
            );
            environment.values.extend(inputs.values);
            environment.redactions.extend(inputs.redactions);
            ensure!(
                environment.values.values().map(String::len).sum::<usize>() <= 128 * 1024,
                "resolved environment too large"
            );
        }
        if !has_receipt {
            environment
                .values
                .extend(self.workflow_environment(&job).await?);
            environment.values.insert(
                "CONVERYN_OWNER_UID".into(),
                job.owner_uid.clone().unwrap_or_default(),
            );
            environment
                .values
                .insert("CONVERYN_NODE_UID".into(), self.cfg.node_uid.clone());
            environment
                .values
                .insert("CONVERYN_RUN_ID".into(), job.run_id.clone());
            environment
                .values
                .insert("CONVERYN_WORK_ID".into(), job.work.id.clone());
            let latest = self.attempt(&job).await?;
            if latest["status"] == "cancel_requested" {
                self.report(
                    &job,
                    latest["last_sequence"].as_u64().unwrap_or(0) + 1,
                    "canceled",
                )
                .await?;
                self.clear()?;
                return Ok(());
            }
        }
        // Duplicate sequence 1 is idempotent even after a lost acknowledgment.
        self.report(&job, 1, "running").await?;
        let cancel = Arc::new(AtomicBool::new(attempt["status"] == "cancel_requested"));
        let stop = cancel.clone();
        let policy = if has_receipt {
            None
        } else {
            crate::executors::authorization::policy(
                &self.cfg,
                &job.work.operation,
                job.project.as_deref(),
            )?
        };
        let output_context = self.output_context(&job);
        let mut child = tokio::task::spawn_blocking(move || {
            crate::executors::runner::run_script_options(
                &journal,
                &operation_id,
                &script,
                &digest,
                u64::from(timeout),
                &stop,
                crate::executors::runner::Options {
                    outputs: Some(&output_context),
                    policy: policy.as_ref(),
                    environment: Some(&environment),
                },
            )
        });
        let mut last_heartbeat = tokio::time::Instant::now();
        let receipt = loop {
            tokio::select! {
                result=&mut child=>break result??,
                _=tokio::time::sleep(Duration::from_secs(1))=>{
                    if last_heartbeat.elapsed()>=Duration::from_secs(20) {let _=self.heartbeat().await;last_heartbeat=tokio::time::Instant::now();}
                    if let Ok(a)=self.attempt(&job).await&& a["status"]=="cancel_requested" {cancel.store(true,Ordering::SeqCst);}
                }
            }
        };
        // The runner receipt already exists; persist the outbox before contacting the controller.
        job.receipt = Some(receipt);
        self.save(&job)?;
        Ok(())
    }
}

/// The installer and running supervisor share the same identity-write boundary.
fn protected_paths<'a>(
    cfg: &'a Config,
    config_path: &'a Path,
    executable: &'a Path,
) -> Vec<&'a Path> {
    let mut protected = vec![
        executable,
        config_path,
        Path::new("/etc/converyn"),
        Path::new("/etc/systemd/system/converyn-agent.service"),
        cfg.state_dir.as_path(),
        cfg.token_file.as_path(),
        cfg.tls.key.as_path(),
        cfg.tls.cert.as_path(),
        cfg.tls.ca.as_path(),
    ];
    if let Some(path) = &cfg.hardware_id_file {
        protected.push(path);
    }
    if let Some(execution) = &cfg.execution {
        // The supervisor owns task scripts, journals and immutable dependency
        // packages here; business write grants must not cover this parent.
        protected.push(&execution.work_root);
    }
    for source in cfg.artifact_sources.sources.values() {
        protected.extend(
            source
                .ca_file
                .iter()
                .chain(source.bearer_token_file.iter())
                .map(|p| p.as_path()),
        );
    }
    protected
}
pub fn validate_local_policy(cfg: &Config, config_path: &Path, executable: &Path) -> Result<()> {
    let protected = protected_paths(cfg, config_path, executable);
    if let Some(execution) = &cfg.execution {
        crate::executors::authorization::validate_write_scopes(
            &execution.writable_paths,
            &protected,
        )?;
    }
    Ok(())
}
pub fn validate_native_operation(
    cfg: &Config,
    config_path: &Path,
    executable: &Path,
    operation: &Operation,
    project: Option<&str>,
) -> Result<()> {
    let protected = protected_paths(cfg, config_path, executable);
    let policy = crate::executors::native::managed_policy(operation, project)?;
    policy.validate(&protected)?;
    for (id, grant) in &policy.plugins {
        let paths = crate::plugins::registry()
            .get(id)?
            .policy_paths(&grant.policy)?;
        crate::executors::authorization::validate_write_scopes(&paths.writable, &protected)?;
    }
    Ok(())
}

pub async fn serve(cfg: Config, config_path: &Path) -> Result<()> {
    ensure!(
        cfg.log_max_bytes >= 64 * 1024 * 1024,
        "raw log quota must be at least 64 MiB"
    );
    ensure!(
        cfg.artifact_sources.bytes_per_second != Some(0),
        "artifact rate limit must be positive"
    );
    ensure!(
        converyn_core::model::domain::valid_id(&cfg.node_uid)
            && converyn_core::model::domain::valid_id(&cfg.cluster),
        "invalid identity"
    );
    ensure!(
        !cfg.servers.is_empty() && cfg.servers.len() <= 5,
        "configure 1..5 controllers"
    );
    for base in &cfg.servers {
        let u = reqwest::Url::parse(base)?;
        ensure!(
            u.scheme() == "https"
                && u.host().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.path() == "/"
                && u.query().is_none()
                && u.fragment().is_none(),
            "controllers must be HTTPS origins"
        );
    }
    // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
    if unsafe { libc::geteuid() } == 0 {
        ensure!(
            cfg.execution.is_some(),
            "root Agents require an explicit unprivileged execution policy"
        );
    }
    if let Some(policy) = &cfg.execution {
        policy.validate()?;
    }
    fs::create_dir_all(&cfg.state_dir)?;
    ensure!(
        fs::symlink_metadata(&cfg.state_dir)?.is_dir(),
        "state directory must not be symlink"
    );
    fs::set_permissions(&cfg.state_dir, fs::Permissions::from_mode(0o700))?;
    crate::credentials::security::private_state_directory(&fs::canonicalize(&cfg.state_dir)?)?;
    let executable = std::env::current_exe()?;
    validate_local_policy(&cfg, config_path, &executable)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cfg.state_dir.join("agent.lock"))?;
    lock.try_lock_exclusive()?;
    let identity = cfg.state_dir.join("identity.json");
    let binding = json!({"node_uid":cfg.node_uid,"cluster":cfg.cluster});
    if identity.exists() {
        ensure!(
            serde_json::from_slice::<Value>(&fs::read(&identity)?)? == binding,
            "agent directory identity mismatch"
        );
    } else {
        durable(&identity, &serde_json::to_vec(&binding)?)?;
    }
    let session = random_id()?;
    let active = crate::nodes::enrollment::active(&cfg)?;
    let tls = active.as_ref().map(|c| &c.tls).unwrap_or(&cfg.tls);
    let token_file = active
        .as_ref()
        .map(|c| &c.token_file)
        .unwrap_or(&cfg.token_file);
    let api = Client {
        preferred: Default::default(),
        fingerprint: Arc::new(std::sync::RwLock::new(Some(
            crate::credentials::pki::info(&fs::read_to_string(&tls.cert)?)?.fingerprint,
        ))),
        hardware: crate::executors::host::hardware(cfg.hardware_id_file.as_deref())?,
        session: Some(session.clone()),
        http: Arc::new(std::sync::RwLock::new(tls.client()?)),
        servers: cfg.servers.clone(),
        token: crate::credentials::security::secret(token_file)?,
        cluster: cfg.cluster.clone(),
    };
    if cfg.enrolled {
        let config = cfg.clone();
        let client = api.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = crate::nodes::enrollment::renew(&config, &client, false).await {
                    tracing::warn!(error=%e,"certificate renewal deferred; existing identity and execution state retained");
                }
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
    }
    tokio::spawn(crate::nodes::log_upload::run(cfg.clone(), api.clone()));
    let agent = Agent {
        config_path: config_path.to_path_buf(),
        telemetry: super::inventory::start(),
        inventory_sent: std::sync::Mutex::new(None),
        cfg,
        api,
        boot: fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .into(),
        session,
    };
    // Spreading startup and retries avoids synchronized Agent cohorts after a
    // rack power recovery; identities and execution grants remain unchanged.
    let jitter = Duration::from_millis(u64::from_str_radix(&agent.session[..4], 16)? % 1000);
    tokio::time::sleep(jitter).await;
    let mut ready = false;
    let mut delay = 1;
    let mut last_heartbeat = tokio::time::Instant::now() - Duration::from_secs(30);
    let mut last_retention = tokio::time::Instant::now() - Duration::from_secs(60);
    loop {
        if last_heartbeat.elapsed() >= Duration::from_secs(20) {
            ready = match agent.heartbeat().await {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(error=%e,"heartbeat/session registration deferred; no new execution started");
                    let _ = agent.local_status(false, Some(e.to_string()), 2);
                    false
                }
            };
            if ready {
                last_heartbeat = tokio::time::Instant::now();
            }
        }
        if !ready {
            tokio::time::sleep(Duration::from_secs(2) + jitter).await;
            continue;
        }
        if last_retention.elapsed() >= Duration::from_secs(60) {
            let path = agent.cfg.state_dir.clone();
            let limit = agent.cfg.log_max_bytes;
            match tokio::task::spawn_blocking(move || {
                crate::executors::log_retention::collect_agent(&path, limit)
            })
            .await
            {
                Ok(Ok(_)) => last_retention = tokio::time::Instant::now(),
                result => {
                    tracing::warn!(
                        ?result,
                        "log retention deferred; execution receipts preserved"
                    );
                    last_retention = tokio::time::Instant::now();
                }
            }
        }
        match agent.step().await {
            Ok(()) => delay = 1,
            Err(e) => {
                tracing::warn!(error=%e,"agent request/execution deferred; durable intent retained");
                delay = (delay * 2).min(30);
                let _ = agent.local_status(ready, Some(e.to_string()), delay);
            }
        }
        tokio::time::sleep(Duration::from_secs(delay) + jitter).await;
    }
}

/// Offline one-time config migration. Preserve the original file and all node
/// identity selectors; only obsolete local authorization fields are removed.
pub fn migrate_config(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(metadata.is_file(), "Agent config must be a regular file");
    let bytes = std::fs::read(path)?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let object = value
        .as_object_mut()
        .context("Agent config must be an object")?;
    let changed = ["native_policy", "execution_profiles", "secrets"]
        .into_iter()
        .map(|k| object.remove(k).is_some())
        .fold(false, |a, b| a | b);
    let config: Config = serde_json::from_value(value.clone())?;
    let lock_path = config.state_dir.join("agent.lock");
    let _lock = if lock_path.exists() {
        let lock = OpenOptions::new().read(true).write(true).open(lock_path)?;
        lock.try_lock_exclusive()
            .context("stop the Agent before migrating its configuration")?;
        Some(lock)
    } else {
        None
    };
    if !changed {
        return Ok(());
    }
    let backup = path.with_extension("before-managed.json");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&backup)
        .context("Agent config backup exists; inspect it before retrying")?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    durable(path, &serde_json::to_vec_pretty(&value)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::os::unix::fs::chown(path, Some(metadata.uid()), Some(metadata.gid()))?;
        std::fs::set_permissions(path, metadata.permissions())?;
    }
    eprintln!(
        "Agent config migrated; original preserved at {}",
        backup.display()
    );
    Ok(())
}
