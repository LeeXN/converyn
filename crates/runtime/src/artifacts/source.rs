//! Named artifact origins keep credentials out of immutable Plans and Raft logs.
//! A source credential is only sent to its exact configured HTTPS origin.
use anyhow::{Context, Result, ensure};
pub use converyn_core::model::artifact::{ArtifactRef, Location, validate_path};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub origin: String,
    #[serde(default)]
    pub ca_file: Option<PathBuf>,
    #[serde(default)]
    pub bearer_token_file: Option<PathBuf>,
}
impl Source {
    fn origin(&self) -> Result<reqwest::Url> {
        let u = reqwest::Url::parse(&self.origin)?;
        ensure!(
            u.scheme() == "https"
                && u.host().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.path() == "/"
                && u.query().is_none()
                && u.fragment().is_none(),
            "artifact source must be an HTTPS origin"
        );
        Ok(u)
    }
    pub fn client_and_url(&self, path: &str) -> Result<(reqwest::Client, String)> {
        validate_path(path)?;
        let origin = self.origin()?;
        let url = origin.join(path)?;
        ensure!(
            url.origin() == origin.origin(),
            "artifact source origin changed"
        );
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30));
        if let Some(ca) = &self.ca_file {
            builder = builder.tls_built_in_root_certs(false);
            for cert in reqwest::Certificate::from_pem_bundle(&std::fs::read(ca)?)? {
                builder = builder.add_root_certificate(cert);
            }
        }
        if let Some(file) = &self.bearer_token_file {
            let token = crate::credentials::security::secret(file)?;
            let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
            value.set_sensitive(true);
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(reqwest::header::AUTHORIZATION, value);
            builder = builder.default_headers(headers);
        }
        Ok((builder.build()?, url.to_string()))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    #[serde(default)]
    pub bytes_per_second: Option<u64>,
    #[serde(default = "default_quota")]
    pub cache_max_bytes: u64,
    #[serde(default)]
    pub sources: BTreeMap<String, Source>,
}
fn default_quota() -> u64 {
    32 * 1024 * 1024 * 1024
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            sources: BTreeMap::new(),
            cache_max_bytes: default_quota(),
            bytes_per_second: None,
        }
    }
}

impl Sources {
    pub async fn fetch(
        &self,
        artifact: &ArtifactRef,
        cache: &Path,
    ) -> Result<crate::artifacts::artifact::Download> {
        artifact.validate()?;
        use fs2::FileExt;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::create_dir_all(cache)?;
        ensure!(
            std::fs::symlink_metadata(cache)?.is_dir(),
            "artifact cache must be a directory, not a symlink"
        );
        std::fs::set_permissions(cache, std::fs::Permissions::from_mode(0o700))?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(cache.join("quota.lock"))?;
        lock.try_lock_exclusive()
            .context("artifact cache currently reserved")?;
        crate::artifacts::cache::reserve(
            cache,
            &artifact.sha256,
            artifact.size,
            self.cache_max_bytes,
        )?;
        let mut errors = Vec::new();
        for location in &artifact.locations {
            let source = self
                .sources
                .get(&location.source)
                .context("artifact source is not configured on this Agent")?;
            let (client, url) = source.client_and_url(&location.path)?;
            match crate::artifacts::artifact::fetch_with_client(
                &client,
                &url,
                crate::artifacts::artifact::Artifact {
                    sha256: artifact.sha256.clone(),
                    size: artifact.size,
                },
                cache,
                4,
                self.bytes_per_second,
            )
            .await
            {
                Ok(download) => return Ok(download),
                Err(_) => errors.push(location.source.clone()),
            }
        }
        anyhow::bail!(
            "artifact sources failed: {} (credentials and URLs omitted)",
            errors.join(", ")
        )
    }
}
