//! Immutable, content-checked log objects. Raw output never enters Raft.
//! S3 uses SigV4 and conditional puts; filesystem publication uses hard links.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Write, path::PathBuf};
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum Config {
    Filesystem {
        directory: PathBuf,
        #[serde(default)]
        shared: bool,
        #[serde(default = "retention")]
        retention_days: u32,
    },
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        prefix: String,
        access_key_file: PathBuf,
        secret_key_file: PathBuf,
        #[serde(default)]
        session_token_file: Option<PathBuf>,
        #[serde(default = "retention")]
        retention_days: u32,
    },
}
fn retention() -> u32 {
    30
}
#[derive(Clone)]
pub struct Store {
    pub config: Config,
    pub id: String,
    client: reqwest::Client,
}
impl Config {
    pub fn resolve(&mut self, base: &std::path::Path) {
        let resolve = |p: &mut PathBuf| {
            if p.is_relative() {
                *p = base.join(&*p)
            }
        };
        match self {
            Self::Filesystem { directory, .. } => resolve(directory),
            Self::S3 {
                access_key_file,
                secret_key_file,
                session_token_file,
                ..
            } => {
                resolve(access_key_file);
                resolve(secret_key_file);
                if let Some(p) = session_token_file {
                    resolve(p)
                }
            }
        }
    }
}
impl Store {
    pub async fn open(config: Config) -> Result<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let mut store = Self {
            config,
            id: String::new(),
            client,
        };
        match &store.config {
            Config::Filesystem {
                directory,
                retention_days,
                ..
            } => {
                ensure!(
                    directory.is_absolute() && *retention_days >= 30,
                    "absolute log directory and retention >=30 days required"
                );
                let root = directory.clone();
                file_io(move || {
                    use std::os::unix::fs::DirBuilderExt;
                    std::fs::DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(&root)?;
                    crate::credentials::security::private_state_directory(&root)?;
                    ensure!(
                        !std::fs::symlink_metadata(root)?.file_type().is_symlink(),
                        "log root must not be a symlink"
                    );
                    Ok(())
                })
                .await?;
            }
            Config::S3 {
                endpoint,
                bucket,
                prefix,
                region,
                retention_days,
                ..
            } => {
                let u = reqwest::Url::parse(endpoint)?;
                ensure!(
                    (u.scheme() == "https"
                        || (u.scheme() == "http"
                            && u.host_str().is_some_and(|h| h
                                .parse::<std::net::IpAddr>()
                                .is_ok_and(|ip| ip.is_loopback()))))
                        && u.username().is_empty()
                        && u.password().is_none()
                        && u.query().is_none()
                        && u.path() == "/"
                        && u.fragment().is_none(),
                    "S3 requires HTTPS origin or numeric loopback fixture"
                );
                ensure!(
                    safe(bucket)
                        && safe(region)
                        && !prefix.is_empty()
                        && prefix.split('/').all(safe)
                        && *retention_days >= 30,
                    "invalid S3 log namespace or retention"
                );
            }
        }
        let marker = serde_json::json!({"format":"converyn-log-store-v1","id":crate::credentials::pki::random_token()?});
        store
            .put("storage-id", &serde_json::to_vec(&marker)?)
            .await?;
        let marker: serde_json::Value = serde_json::from_slice(
            &store
                .get("storage-id")
                .await?
                .context("storage marker missing")?,
        )?;
        ensure!(
            marker["format"] == "converyn-log-store-v1",
            "not a Converyn log storage namespace"
        );
        store.id = marker["id"]
            .as_str()
            .filter(|id| converyn_core::identities::registration::valid_sha(id))
            .context("invalid log storage identity")?
            .to_owned();
        Ok(store)
    }
    pub async fn verify_identity(&self) -> Result<bool> {
        let marker: serde_json::Value = serde_json::from_slice(
            &self
                .get("storage-id")
                .await?
                .context("storage marker missing")?,
        )?;
        Ok(marker["format"] == "converyn-log-store-v1" && marker["id"] == self.id)
    }
    pub fn shared(&self) -> bool {
        !matches!(self.config, Config::Filesystem { shared: false, .. })
    }
    pub fn retention_days(&self) -> u32 {
        match &self.config {
            Config::Filesystem { retention_days, .. } | Config::S3 { retention_days, .. } => {
                *retention_days
            }
        }
    }
    async fn request(
        &self,
        method: reqwest::Method,
        key: &str,
        body: &[u8],
        create: bool,
    ) -> Result<reqwest::Response> {
        let Config::S3 {
            endpoint,
            bucket,
            region,
            prefix,
            access_key_file,
            secret_key_file,
            session_token_file,
            ..
        } = &self.config
        else {
            anyhow::bail!("S3 operation on filesystem")
        };
        ensure!(key.split('/').all(safe), "invalid log object key");
        let url = reqwest::Url::parse(&format!("{endpoint}{bucket}/{prefix}/{key}"))?;
        let now = time::OffsetDateTime::now_utc();
        let date = format!(
            "{:04}{:02}{:02}",
            now.year(),
            u8::from(now.month()),
            now.day()
        );
        let stamp = format!(
            "{date}T{:02}{:02}{:02}Z",
            now.hour(),
            now.minute(),
            now.second()
        );
        let host = url.host_str().context("S3 host missing")?;
        let host = match url.port() {
            Some(p) => format!("{host}:{p}"),
            None => host.to_owned(),
        };
        let payload = hex::encode(Sha256::digest(body));
        let token = session_token_file
            .as_ref()
            .map(|p| crate::credentials::security::secret(p))
            .transpose()?;
        let mut canonical =
            format!("host:{host}\nx-amz-content-sha256:{payload}\nx-amz-date:{stamp}\n");
        let mut names = "host;x-amz-content-sha256;x-amz-date".to_string();
        if let Some(token) = &token {
            ensure!(!token.contains(['\r', '\n']), "invalid session token");
            canonical.push_str(&format!("x-amz-security-token:{token}\n"));
            names.push_str(";x-amz-security-token");
        }
        let canonical = format!(
            "{method}\n{}\n\n{canonical}\n{names}\n{payload}",
            url.path()
        );
        let scope = format!("{date}/{region}/s3/aws4_request");
        let sign = format!(
            "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        );
        fn hmac(key: &[u8], data: &str) -> Vec<u8> {
            ring::hmac::sign(
                &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
                data.as_bytes(),
            )
            .as_ref()
            .to_vec()
        }
        let secret = crate::credentials::security::secret(secret_key_file)?;
        let key = hmac(format!("AWS4{secret}").as_bytes(), &date);
        let key = hmac(&key, region);
        let key = hmac(&key, "s3");
        let key = hmac(&key, "aws4_request");
        let access = crate::credentials::security::secret(access_key_file)?;
        ensure!(
            !access.contains(['\r', '\n'])
                && access
                    .bytes()
                    .all(|b| b.is_ascii_graphic() && !b"/,".contains(&b)),
            "invalid access ID"
        );
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders={names}, Signature={}",
            hex::encode(hmac(&key, &sign))
        );
        let mut request = self
            .client
            .request(method, url)
            .header("x-amz-content-sha256", payload)
            .header("x-amz-date", stamp)
            .header("authorization", authorization);
        if let Some(token) = token {
            request = request.header("x-amz-security-token", token)
        }
        if create {
            request = request.header("if-none-match", "*")
        }
        Ok(request
            .body(body.to_vec())
            .send()
            .await
            .map_err(|e| e.without_url())?)
    }
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        ensure!(key.split('/').all(safe), "invalid log object key");
        match &self.config {
            Config::Filesystem { directory, .. } => {
                let root = directory.clone();
                let path = root.join(key);
                file_io(move || {
                    use std::io::Read;
                    let meta = match std::fs::symlink_metadata(&path) {
                        Ok(v) => v,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                        Err(e) => return Err(e.into()),
                    };
                    ensure!(
                        meta.is_file()
                            && meta.len() <= 1024 * 1024
                            && path.canonicalize()?.starts_with(root.canonicalize()?),
                        "invalid log object path or size"
                    );
                    let mut data = Vec::new();
                    std::fs::File::open(path)?
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut data)?;
                    ensure!(data.len() <= 1024 * 1024, "log object exceeds limit");
                    Ok(Some(data))
                })
                .await
            }
            Config::S3 { .. } => {
                let mut response = self.request(reqwest::Method::GET, key, &[], false).await?;
                if response.status() == 404 {
                    return Ok(None);
                }
                ensure!(
                    response.status().is_success(),
                    "S3 read failed: {}",
                    response.status()
                );
                ensure!(
                    response.content_length().is_none_or(|n| n <= 1024 * 1024),
                    "log object exceeds limit"
                );
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    ensure!(
                        bytes.len() + chunk.len() <= 1024 * 1024,
                        "log object exceeds limit"
                    );
                    bytes.extend_from_slice(&chunk);
                }
                Ok(Some(bytes))
            }
        }
    }
    /// Replace only the bounded, explicitly transient preview object. Final chunks
    /// and manifests still use immutable put().
    pub async fn put_live(&self, key: &str, body: &[u8]) -> Result<()> {
        ensure!(
            key.starts_with("logs/")
                && key.ends_with("/live")
                && key.split('/').all(safe)
                && body.len() <= 96 * 1024,
            "invalid live object"
        );
        match &self.config {
            Config::Filesystem { directory, .. } => {
                let root = directory.clone();
                let destination = root.join(key);
                let bytes = body.to_vec();
                file_io(move || {
                    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
                    let parent = destination.parent().unwrap();
                    std::fs::DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(parent)?;
                    ensure!(
                        parent.canonicalize()?.starts_with(root.canonicalize()?),
                        "live path escaped storage root"
                    );
                    let temporary =
                        parent.join(format!("live-{}", crate::credentials::pki::random_token()?));
                    let mut f = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&temporary)?;
                    f.write_all(&bytes)?;
                    f.sync_all()?;
                    std::fs::rename(&temporary, &destination)?;
                    std::fs::File::open(parent)?.sync_all()?;
                    Ok(())
                })
                .await
            }
            Config::S3 { .. } => {
                let response = self.request(reqwest::Method::PUT, key, body, false).await?;
                ensure!(
                    response.status().is_success(),
                    "live S3 write failed: {}",
                    response.status()
                );
                Ok(())
            }
        }
    }
    pub async fn put(&self, key: &str, body: &[u8]) -> Result<()> {
        ensure!(
            key.split('/').all(safe) && body.len() <= 1024 * 1024,
            "invalid object key or size"
        );
        match &self.config {
            Config::Filesystem { directory, .. } => {
                let root = directory.clone();
                let destination = root.join(key);
                let bytes = body.to_vec();
                file_io(move || {
                    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
                    let parent = destination.parent().unwrap();
                    std::fs::DirBuilder::new()
                        .recursive(true)
                        .mode(0o700)
                        .create(parent)?;
                    ensure!(
                        parent.canonicalize()?.starts_with(root.canonicalize()?),
                        "log path escaped storage root"
                    );
                    let temporary = parent.join(format!(
                        "upload-{}",
                        crate::credentials::pki::random_token()?
                    ));
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&temporary)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    drop(file);
                    let publish = std::fs::hard_link(&temporary, &destination);
                    let _ = std::fs::remove_file(&temporary);
                    match publish {
                        Ok(()) => std::fs::File::open(parent)?.sync_all()?,
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.into()),
                    }
                    Ok(())
                })
                .await?;
            }
            Config::S3 { .. } => {
                let response = self.request(reqwest::Method::PUT, key, body, true).await?;
                ensure!(
                    response.status().is_success()
                        || response.status() == 412
                        || response.status() == 409,
                    "S3 conditional put failed: {}",
                    response.status()
                );
            }
        }
        // The marker is the only intentionally first-writer-chosen value.
        if key != "storage-id" {
            ensure!(
                self.get(key).await?.as_deref() == Some(body),
                "immutable log object conflict"
            )
        }
        Ok(())
    }
    pub async fn delete(&self, key: &str) -> Result<()> {
        ensure!(key.split('/').all(safe), "invalid log object key");
        match &self.config {
            Config::Filesystem { directory, .. } => {
                let root = directory.clone();
                let path = root.join(key);
                file_io(move || {
                    if let Some(parent) = path.parent()
                        && parent.exists()
                    {
                        ensure!(
                            parent.canonicalize()?.starts_with(root.canonicalize()?),
                            "log delete escaped storage root"
                        );
                    }
                    match std::fs::remove_file(path) {
                        Ok(()) => Ok(()),
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                })
                .await
            }
            Config::S3 { .. } => {
                let response = self
                    .request(reqwest::Method::DELETE, key, &[], false)
                    .await?;
                ensure!(
                    response.status().is_success() || response.status() == 404,
                    "S3 delete failed"
                );
                Ok(())
            }
        }
    }
}
fn safe(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

// A wedged NFS mount must neither occupy async workers nor exhaust Tokio's
// shared blocking pool used by the local Raft database. The permit belongs to
// the actual blocking operation, even if its caller times out or disconnects.
static FILE_IO: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();
async fn file_io<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async move {
        let permit = FILE_IO
            .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(16)))
            .clone()
            .acquire_owned()
            .await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        })
        .await?
    })
    .await
    .context("log filesystem I/O timed out; operation may still be in flight")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn blocked_log_io_leaves_async_worker_live_and_keeps_its_permit_after_disconnect() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(file_io(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv()?;
            Ok(())
        }));
        tokio::time::timeout(std::time::Duration::from_secs(1), entered_rx)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        let _ = task.await;
        assert_eq!(FILE_IO.get().unwrap().available_permits(), 15);
        release_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while FILE_IO.get().unwrap().available_permits() != 16 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
