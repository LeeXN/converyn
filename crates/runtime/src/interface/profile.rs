//! A portable client file contains only personal credentials, never an issuer key.
use super::client::{Config, validate_servers};
use crate::credentials::security::{Tls, secret};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub ca_pem: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub token: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    pub cluster: String,
    pub servers: Vec<String>,
    pub credentials: Credentials,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub current_context: Option<String>,
    pub contexts: BTreeMap<String, ContextConfig>,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            version: 1,
            current_context: None,
            contexts: BTreeMap::new(),
        }
    }
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.@".contains(&b))
}
impl ContextConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            converyn_core::model::domain::valid_id(&self.cluster),
            "invalid control-plane name"
        );
        validate_servers(&self.servers)?;
        ensure!(
            self.credentials.token.len() >= 32
                && self.credentials.token.bytes().all(|b| b.is_ascii_graphic()),
            "invalid personal token"
        );
        ensure!(
            !self.credentials.ca_pem.contains("PRIVATE KEY")
                && !self.credentials.certificate_pem.contains("PRIVATE KEY"),
            "public certificate fields must not contain private keys"
        );
        // Parse and pair the key/certificate now so a failed import leaves the file untouched.
        Tls::client_pem(
            self.credentials.certificate_pem.as_bytes(),
            self.credentials.private_key_pem.as_bytes(),
            self.credentials.ca_pem.as_bytes(),
        )?;
        Ok(())
    }
    pub fn client_config(&self) -> Config {
        Config {
            cluster: self.cluster.clone(),
            servers: self.servers.clone(),
            token_file: PathBuf::new(),
            tls: Tls {
                cert: PathBuf::new(),
                key: PathBuf::new(),
                ca: PathBuf::new(),
            },
            embedded: Some(self.credentials.clone()),
        }
    }
    pub fn from_config(config: Config) -> Result<Self> {
        let credentials = match config.embedded {
            Some(c) => c,
            None => Credentials {
                ca_pem: std::fs::read_to_string(config.tls.ca)?,
                certificate_pem: std::fs::read_to_string(config.tls.cert)?,
                private_key_pem: secret(&config.tls.key)?,
                token: secret(&config.token_file)?,
            },
        };
        let context = Self {
            cluster: config.cluster,
            servers: config.servers,
            credentials,
        };
        context.validate()?;
        Ok(context)
    }
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "unsupported client config version");
        ensure!(
            self.contexts.len() <= 128,
            "client config has too many contexts"
        );
        if let Some(name) = &self.current_context {
            ensure!(
                self.contexts.contains_key(name),
                "current context is missing"
            );
        }
        for (name, context) in &self.contexts {
            ensure!(valid_name(name), "invalid context name");
            context
                .validate()
                .with_context(|| format!("invalid context {name}"))?;
        }
        Ok(())
    }
    pub fn select(&self, name: Option<&str>) -> Result<(&str, &ContextConfig)> {
        let name = name
            .or(self.current_context.as_deref())
            .context("no current context; run `converyn config use-context NAME`")?;
        self.contexts
            .get_key_value(name)
            .map(|(n, c)| (n.as_str(), c))
            .with_context(|| format!("context {name} not found"))
    }
}
fn read_json(path: &Path) -> Result<(serde_json::Value, std::fs::Metadata)> {
    let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path)
        .with_context(|| format!("cannot read config {}; import personal credentials with `converyn config import FILE --name NAME`", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "config must be a regular file");
    let mut bytes = Vec::new();
    file.by_ref()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "client config exceeds 2 MiB"
    );
    // serde_json errors contain location/type information, never print the input document.
    let value = serde_json::from_slice(&bytes).context("invalid client JSON config")?;
    Ok((value, metadata))
}
pub fn read_profile(path: &Path) -> Result<Profile> {
    let (value, metadata) = read_json(path)?;
    // SAFETY: geteuid reads the effective identity without changing process state.
    ensure!(
        metadata.mode() & 0o077 == 0 && metadata.uid() == unsafe { libc::geteuid() },
        "personal config must be owned by the current user with mode 0600"
    );
    let profile: Profile =
        serde_json::from_value(value).context("invalid context config structure")?;
    profile.validate()?;
    Ok(profile)
}
pub fn load(path: &Path, context: Option<&str>) -> Result<Config> {
    Ok(read_profile(path)?.select(context)?.1.client_config())
}
/// Persist the same portable profile used by imports and normal CLI commands.
pub fn write(path: &Path, name: &str, config: Config) -> Result<()> {
    let profile = Profile {
        version: 1,
        current_context: Some(name.into()),
        contexts: BTreeMap::from([(name.into(), ContextConfig::from_config(config)?)]),
    };
    profile.validate()?;
    crate::credentials::pki::private(path, &serde_json::to_vec_pretty(&profile)?)
}
