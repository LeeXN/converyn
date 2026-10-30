use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use futures_util::StreamExt;
use reqwest::{
    StatusCode,
    header::{ACCEPT_ENCODING, CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use converyn_core::model::domain::MAX_ARTIFACT_SIZE;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct Download {
    pub path: PathBuf,
    pub bytes: u64,
    pub resumed_from: u64,
    pub cache_hit: bool,
    pub attempts: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("temporary upstream error: {0}")]
struct Retryable(String);

fn open_private(path: &Path) -> Result<File> {
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    Ok(opts.open(path)?)
}

pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    hash_file_with_progress(path, |_, _| {})
}

pub fn hash_file_with_progress(
    path: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<(String, u64)> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts.open(path)?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "artifact is not a regular file");
    let total = metadata.len();
    let mut buffer = vec![0; 256 * 1024];
    let mut digest = Sha256::new();
    let mut count = 0;
    progress(0, total);
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
        count += n as u64;
        progress(count, total);
    }
    Ok((hex::encode(digest.finalize()), count))
}

async fn verify(path: PathBuf, spec: Artifact) -> Result<()> {
    let (hash, size) = tokio::task::spawn_blocking(move || hash_file(&path)).await??;
    ensure!(
        hash == spec.sha256 && size == spec.size,
        "artifact checksum or size mismatch"
    );
    Ok(())
}

pub async fn fetch(
    url: &str,
    spec: Artifact,
    cache: &Path,
    attempts: u32,
    bytes_per_second: Option<u64>,
) -> Result<Download> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    fetch_with_client(&client, url, spec, cache, attempts, bytes_per_second).await
}

pub(crate) async fn fetch_with_client(
    client: &reqwest::Client,
    url: &str,
    spec: Artifact,
    cache: &Path,
    attempts: u32,
    bytes_per_second: Option<u64>,
) -> Result<Download> {
    fetch_with_headers(
        client,
        url,
        spec,
        cache,
        attempts,
        bytes_per_second,
        &reqwest::header::HeaderMap::new(),
    )
    .await
}

pub(crate) async fn fetch_with_headers(
    client: &reqwest::Client,
    url: &str,
    spec: Artifact,
    cache: &Path,
    attempts: u32,
    bytes_per_second: Option<u64>,
    headers: &reqwest::header::HeaderMap,
) -> Result<Download> {
    ensure!(
        spec.size > 0 && spec.size <= MAX_ARTIFACT_SIZE,
        "artifact size must be 1..=10 GiB"
    );
    ensure!(
        spec.sha256.len() == 64
            && spec
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid lowercase SHA256"
    );
    ensure!((1..=8).contains(&attempts), "attempts must be 1..=8");
    ensure!(bytes_per_second != Some(0), "rate limit must be positive");
    let url = reqwest::Url::parse(url)?;
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not accepted"
    );
    ensure!(
        url.scheme() == "https"
            || (url.scheme() == "http"
                && url.host_str().is_some_and(|s| s
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback()))),
        "HTTPS required except loopback test sources"
    );
    ensure!(
        !std::fs::symlink_metadata(cache).is_ok_and(|m| m.file_type().is_symlink()),
        "cache directory must not be a symlink"
    );
    std::fs::create_dir_all(cache)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cache, std::fs::Permissions::from_mode(0o700))?;
    }
    let _lock = open_private(&cache.join(format!("{}.lock", spec.sha256)))?;
    _lock
        .try_lock_exclusive()
        .context("another download owns this artifact")?;
    let target = cache.join(&spec.sha256);
    if target.try_exists()? {
        verify(target.clone(), spec.clone()).await?;
        return Ok(Download {
            path: target,
            bytes: spec.size,
            resumed_from: 0,
            cache_hit: true,
            attempts: 0,
        });
    }
    for attempt in 1..=attempts {
        match once(client, &url, &spec, cache, bytes_per_second, headers).await {
            Ok(mut r) => {
                r.attempts = attempt;
                return Ok(r);
            }
            Err(e) => {
                let retryable = e.downcast_ref::<Retryable>().is_some()
                    || e.downcast_ref::<reqwest::Error>().is_some_and(|e| {
                        e.is_connect() || e.is_timeout() || e.is_body() || e.is_request()
                    });
                if !retryable || attempt == attempts {
                    return Err(e);
                }
                tokio::time::sleep(Duration::from_millis(200 * (1u64 << (attempt - 1)).min(16)))
                    .await;
            }
        }
    }
    unreachable!()
}

async fn once(
    client: &reqwest::Client,
    url: &reqwest::Url,
    spec: &Artifact,
    cache: &Path,
    rate: Option<u64>,
    headers: &reqwest::header::HeaderMap,
) -> Result<Download> {
    let part = cache.join(format!("{}.part", spec.sha256));
    let target = cache.join(&spec.sha256);
    let validator_path = cache.join(format!("{}.etag", spec.sha256));
    let file = open_private(&part)?;
    ensure!(
        file.metadata()?.is_file(),
        "partial artifact is not a regular file"
    );
    let mut offset = file.metadata()?.len();
    ensure!(
        offset <= spec.size,
        "partial artifact is larger than expected"
    );
    let mut start_offset = offset;
    if offset < spec.size {
        ensure!(
            fs2::available_space(cache)? >= spec.size - offset + 64 * 1024 * 1024,
            "insufficient space for artifact plus reserve"
        );
        let mut request = client
            .get(url.clone())
            .headers(headers.clone())
            .timeout(Duration::from_secs(3600))
            .header(ACCEPT_ENCODING, "identity");
        let validator = std::fs::read_to_string(&validator_path)
            .ok()
            .filter(|s| s.starts_with('"') && s.ends_with('"') && s.len() <= 1024);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
            if let Some(v) = &validator {
                request = request.header(IF_RANGE, v);
            }
        }
        let response = request.send().await.map_err(|e| e.without_url())?;
        if response.status().is_server_error() || response.status() == StatusCode::TOO_MANY_REQUESTS
        {
            return Err(Retryable(response.status().to_string()).into());
        }
        if response.status() == StatusCode::OK {
            // A server ignoring Range returns a full representation: replace, never append.
            file.set_len(0)?;
            offset = 0;
            start_offset = 0;
        } else if response.status() == StatusCode::PARTIAL_CONTENT {
            let range = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .context("missing Content-Range")?;
            let values = range
                .strip_prefix("bytes ")
                .context("invalid Content-Range")?;
            let (span, total) = values.split_once('/').context("invalid Content-Range")?;
            let (from, to) = span.split_once('-').context("invalid Content-Range")?;
            ensure!(
                from.parse::<u64>()? == offset
                    && total.parse::<u64>()? == spec.size
                    && to.parse::<u64>()? == spec.size - 1,
                "unexpected Content-Range"
            );
            if let (Some(old), Some(new)) = (
                validator.as_ref(),
                response.headers().get(ETAG).and_then(|v| v.to_str().ok()),
            ) && old != new
            {
                file.set_len(0)?;
                bail!("source validator changed during partial response");
            }
        } else {
            bail!("artifact source returned {}", response.status());
        }
        if let Some(length) = response.content_length() {
            ensure!(length == spec.size - offset, "unexpected Content-Length");
        }
        if let Some(etag) = response
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.starts_with('"') && v.ends_with('"') && v.len() <= 1024)
        {
            use std::io::Write;
            let mut f = open_private(&validator_path)?;
            f.set_len(0)?;
            f.write_all(etag.as_bytes())?;
            f.sync_all()?;
        }
        let mut out = tokio::fs::File::from_std(file);
        out.seek(std::io::SeekFrom::Start(offset)).await?;
        let mut stream = response.bytes_stream();
        let started = Instant::now();
        let base = offset;
        let mut last_sync = offset;
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    out.sync_all().await?;
                    return Err(e.without_url().into());
                }
            };
            ensure!(
                chunk.len() as u64 <= spec.size - offset,
                "source exceeded declared size"
            );
            out.write_all(&chunk).await?;
            offset += chunk.len() as u64;
            if offset - last_sync >= 8 * 1024 * 1024 {
                out.sync_data().await?;
                last_sync = offset;
            }
            if let Some(rate) = rate {
                let expected = Duration::from_secs_f64((offset - base) as f64 / rate as f64);
                tokio::time::sleep(expected.saturating_sub(started.elapsed())).await;
            }
        }
        out.sync_all().await?;
        if offset != spec.size {
            return Err(Retryable("incomplete body".into()).into());
        }
        drop(out);
    } else {
        drop(file);
    }
    if let Err(e) = verify(part.clone(), spec.clone()).await {
        std::fs::remove_file(&part)?;
        return Err(e);
    }
    // A complete .part may originate from a killed process whose last write was not synced.
    open_private(&part)?.sync_all()?;
    std::fs::rename(&part, &target)?;
    File::open(cache)?.sync_all()?;
    Ok(Download {
        path: target,
        bytes: spec.size,
        resumed_from: start_offset,
        cache_hit: false,
        attempts: 1,
    })
}
