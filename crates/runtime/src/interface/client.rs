use crate::credentials::security::Tls;
use crate::nodes::agent::Client;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub servers: Vec<String>,
    pub cluster: String,
    #[serde(default)]
    pub token_file: PathBuf,
    #[serde(default = "empty_tls")]
    pub tls: Tls,
    #[serde(skip)]
    pub embedded: Option<super::profile::Credentials>,
}
fn empty_tls() -> Tls {
    Tls {
        cert: PathBuf::new(),
        key: PathBuf::new(),
        ca: PathBuf::new(),
    }
}
pub fn validate_servers(servers: &[String]) -> Result<()> {
    ensure!(
        !servers.is_empty() && servers.len() <= 5,
        "configure 1..5 controllers"
    );
    for base in servers {
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
    Ok(())
}
impl Config {
    pub fn ca_pem(&self) -> Result<String> {
        match &self.embedded {
            Some(credentials) => Ok(credentials.ca_pem.clone()),
            None => Ok(std::fs::read_to_string(&self.tls.ca)?),
        }
    }
    pub fn client(self) -> Result<Client> {
        validate_servers(&self.servers)?;
        let fingerprint = self
            .embedded
            .as_ref()
            .map(|c| crate::credentials::pki::info(&c.certificate_pem).map(|c| c.fingerprint))
            .transpose()?;
        ensure!(
            !self.servers.is_empty() && self.servers.len() <= 5,
            "configure 1..5 controllers"
        );
        for url in &self.servers {
            let u = reqwest::Url::parse(url)?;
            ensure!(
                u.scheme() == "https"
                    && u.username().is_empty()
                    && u.password().is_none()
                    && u.path() == "/"
                    && u.query().is_none()
                    && u.fragment().is_none(),
                "configure HTTPS origins only"
            );
        }
        let (http, token) = if let Some(credentials) = self.embedded {
            (
                Tls::client_pem(
                    credentials.certificate_pem.as_bytes(),
                    credentials.private_key_pem.as_bytes(),
                    credentials.ca_pem.as_bytes(),
                )?,
                credentials.token,
            )
        } else {
            (
                self.tls.client()?,
                crate::credentials::security::secret(&self.token_file)?,
            )
        };
        Ok(Client {
            preferred: Default::default(),
            fingerprint: std::sync::Arc::new(std::sync::RwLock::new(fingerprint)),
            hardware: None,
            session: None,
            http: std::sync::Arc::new(std::sync::RwLock::new(http)),
            servers: self.servers,
            token,
            cluster: self.cluster,
        })
    }
}
pub async fn request(
    api: &Client,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Value> {
    for attempt in 0..6 {
        match api.request(method.clone(), path, body).await {
            Ok((s, v)) => {
                ensure!((200..300).contains(&s), "API rejected request ({s}): {v}");
                return Ok(v);
            }
            Err(e)
                if e.downcast_ref::<crate::nodes::agent::AuthenticationRejected>()
                    .is_some() =>
            {
                return Err(e);
            }
            Err(e) if attempt == 5 => return Err(e),
            Err(_) => tokio::time::sleep(Duration::from_millis(200 * (1 << attempt))).await,
        }
    }
    unreachable!()
}
pub async fn backup(api: &Client, output: &Path) -> Result<(String, String)> {
    use futures_util::StreamExt;
    use std::{
        fs::{File, OpenOptions},
        io::Write,
        os::unix::fs::OpenOptionsExt,
    };
    ensure!(!output.exists(), "refusing to replace an existing backup");
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = parent.join(format!(
        ".converyn-backup-{}",
        crate::credentials::pki::random_token()?
    ));
    let result = async {
        let http = api
            .http
            .read()
            .map_err(|_| anyhow::anyhow!("poisoned transport"))?
            .clone();
        for base in &api.servers {
            let response = http
                .get(format!("{base}/api/v1/controlplane/backup"))
                .bearer_auth(&api.token)
                .header("x-converyn-cluster", &api.cluster)
                .timeout(Duration::from_secs(3600))
                .send()
                .await;
            let Ok(response) = response else { continue };
            if !response.status().is_success() {
                continue;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                file.write_all(&chunk?)?;
            }
            file.sync_all()?;
            drop(file);
            let details = crate::storage::backup::inspect(&temporary)?;
            std::fs::hard_link(&temporary, output)?;
            File::open(parent)?.sync_all()?;
            return Ok(details);
        }
        anyhow::bail!("all configured backup endpoints unavailable")
    }
    .await;
    let _ = std::fs::remove_file(temporary);
    result
}
pub async fn wait(api: &Client, uid: &str, timeout: u64) -> Result<(u8, Value)> {
    wait_with(api, uid, timeout, |_| {}).await
}
pub async fn wait_with(
    api: &Client,
    uid: &str,
    timeout: u64,
    mut progress: impl FnMut(&Value),
) -> Result<(u8, Value)> {
    let mut previous = String::new();
    ensure!(
        converyn_core::model::domain::valid_id(uid) && (1..=86400).contains(&timeout),
        "invalid wait target or timeout"
    );
    let path = format!("/api/v1/runs/{uid}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let mut last = json!({"kind":"run","uid":uid,"condition":"waiting"});
    let mut delay = 1;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            last["wait_timed_out"] = true.into();
            return Ok((124, last));
        }
        let query = tokio::time::timeout(
            deadline - now,
            api.request(reqwest::Method::GET, &path, None),
        )
        .await;
        let Ok(query) = query else {
            last["wait_timed_out"] = true.into();
            return Ok((124, last));
        };
        match query {
            Ok((200, value)) => {
                let condition = value["status"]
                    .as_str()
                    .context("wait target not found or response missing condition")?;
                let reason = value.get("diagnostic").cloned().unwrap_or(Value::Null);
                let snapshot =
                    converyn_core::model::domain::digest(&(&value["status"], &value["revision"]));
                if snapshot != previous {
                    progress(&value);
                    previous = snapshot;
                }
                last = value.clone();
                last["condition"] = condition.into();
                last["diagnostic"] = reason;
                delay = 1;
                let code = match condition {
                    "succeeded" => Some(0),
                    "failed" | "canceled" => Some(2),
                    "paused" | "needs_attention" | "blocked" | "disaster_restore_paused" => Some(3),
                    c if c.starts_with("rejected:") => Some(2),
                    _ => None,
                };
                if let Some(code) = code {
                    return Ok((code, last));
                }
            }
            Ok((429 | 503, _)) => {
                delay = (delay * 2).min(10);
            }
            Ok((status, _)) => {
                anyhow::bail!("wait query rejected (HTTP {status}); no new intent was submitted")
            }
            Err(error) => {
                if error
                    .downcast_ref::<crate::nodes::agent::AuthenticationRejected>()
                    .is_some()
                {
                    return Err(error);
                }
                last["diagnostic"] = error.to_string().into();
                delay = (delay * 2).min(10);
            }
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            last["wait_timed_out"] = true.into();
            return Ok((124, last));
        }
        tokio::time::sleep(Duration::from_secs(delay).min(deadline - now)).await;
    }
}
