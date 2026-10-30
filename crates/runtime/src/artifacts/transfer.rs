//! Authenticated upload and download of controller-managed immutable artifacts.
use crate::artifacts::{
    artifact::{Artifact, Download},
    managed,
    source::ArtifactRef,
};
use crate::nodes::agent::Client;
use anyhow::{Context, Result, ensure};
use reqwest::{
    Method,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use std::{path::Path, time::Duration};
use tokio::io::AsyncReadExt;

/// Confirmed transfer stages; Completed is emitted only after durable replica verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Checking,
    Uploading { confirmed_bytes: u64 },
    Verifying,
    Reused,
    Completed,
}

fn headers(api: &Client) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    let mut bearer = HeaderValue::from_str(&format!("Bearer {}", api.token))?;
    bearer.set_sensitive(true);
    headers.insert(AUTHORIZATION, bearer);
    headers.insert("x-converyn-cluster", HeaderValue::from_str(&api.cluster)?);
    headers.insert(
        "x-converyn-protocol",
        HeaderValue::from_str(&crate::controller::network::PROTOCOL_VERSION.to_string())?,
    );
    if let Some(v) = &api.hardware {
        headers.insert("x-converyn-hardware", HeaderValue::from_str(v)?);
    }
    if let Some(v) = &api.session {
        headers.insert("x-converyn-session", HeaderValue::from_str(v)?);
    }
    Ok(headers)
}
async fn post(api: &Client, path: &str, bytes: Option<&[u8]>) -> Result<serde_json::Value> {
    let http = api
        .http
        .read()
        .map_err(|_| anyhow::anyhow!("poisoned HTTP client"))?
        .clone();
    let mut last = String::new();
    for _ in 0..3 {
        for base in &api.servers {
            let mut request = http
                .request(Method::POST, format!("{base}{path}"))
                .headers(headers(api)?)
                .timeout(Duration::from_secs(3600));
            if let Some(bytes) = bytes {
                request = request.body(bytes.to_vec());
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let value: serde_json::Value = response.json().await?;
                    if status.is_success() && value["ok"] == true {
                        return Ok(value);
                    }
                    ensure!(
                        status.as_u16() != 401 && status.as_u16() != 403,
                        "artifact upload permission denied (HTTP {status})"
                    );
                    last = format!("HTTP {status}: {value}");
                }
                Err(error) => last = error.without_url().to_string(),
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    anyhow::bail!("artifact upload deferred: {last}")
}
pub async fn upload(api: &Client, file: &Path, expected: &Artifact) -> Result<()> {
    upload_with_progress(api, file, expected, |_| {}).await
}

pub async fn upload_with_progress(
    api: &Client,
    file: &Path,
    expected: &Artifact,
    mut progress: impl FnMut(Progress),
) -> Result<()> {
    managed::validate(&expected.sha256, expected.size)?;
    let complete = format!(
        "/api/v1/artifacts/{}/complete?size={}",
        expected.sha256, expected.size
    );
    // Idempotent re-registration only rechecks retained replicas, without sending bytes.
    progress(Progress::Checking);
    if post(api, &complete, None).await.is_ok() {
        progress(Progress::Reused);
        return Ok(());
    }
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(file)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() == expected.size,
        "artifact changed after packaging"
    );
    let mut file = tokio::fs::File::from_std(file);
    let mut offset = 0;
    let mut buffer = vec![0; managed::CHUNK];
    progress(Progress::Uploading { confirmed_bytes: 0 });
    while offset < expected.size {
        let size = (expected.size - offset).min(managed::CHUNK as u64) as usize;
        file.read_exact(&mut buffer[..size]).await?;
        let path = format!(
            "/api/v1/artifacts/{}/chunks?size={}&offset={offset}",
            expected.sha256, expected.size
        );
        post(api, &path, Some(&buffer[..size])).await?;
        offset += size as u64;
        progress(Progress::Uploading {
            confirmed_bytes: offset,
        });
    }
    ensure!(
        file.read(&mut buffer[..1]).await? == 0,
        "artifact grew after packaging"
    );
    progress(Progress::Verifying);
    post(api, &complete, None).await?;
    progress(Progress::Completed);
    Ok(())
}
pub(crate) async fn download(
    api: &Client,
    artifact: &ArtifactRef,
    cache: &Path,
    path: &str,
    quota: u64,
    rate: Option<u64>,
) -> Result<Download> {
    artifact.validate()?;
    ensure!(
        artifact.locations.is_empty(),
        "managed artifact has external locations"
    );
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;
    managed::directory(cache)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cache.join("quota.lock"))?;
    lock.try_lock_exclusive().context("artifact cache busy")?;
    crate::artifacts::cache::reserve(cache, &artifact.sha256, artifact.size, quota)?;
    let http = api
        .http
        .read()
        .map_err(|_| anyhow::anyhow!("poisoned HTTP client"))?
        .clone();
    let headers = headers(api)?;
    for base in &api.servers {
        let result = crate::artifacts::artifact::fetch_with_headers(
            &http,
            &format!("{base}{path}"),
            Artifact {
                sha256: artifact.sha256.clone(),
                size: artifact.size,
            },
            cache,
            2,
            rate,
            &headers,
        )
        .await;
        if let Ok(download) = result {
            return Ok(download);
        }
    }
    anyhow::bail!("managed artifact download unavailable on configured controllers")
}
