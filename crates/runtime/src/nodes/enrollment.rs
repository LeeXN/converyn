//! Operator-created one-use bootstrap bundles; Node-generated private keys and
//! tokens survive request retries locally and are never sent to the controller.
use crate::credentials::pki;
use crate::credentials::security::Tls;
use crate::nodes::agent::{Client, Config};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapBundle {
    pub cluster: String,
    pub servers: Vec<String>,
    pub node_uid: String,
    pub grant_id: String,
    pub token: String,
    pub private_key_pem: String,
    pub certificate_pem: String,
    pub ca_pem: String,
    pub expires_ms: u64,
}
#[derive(Serialize, Deserialize)]
struct BootstrapPending {
    id: String,
    node_uid: String,
    token: String,
    key: String,
    csr: String,
    ttl: u64,
    recover: bool,
}
#[derive(Serialize, Deserialize)]
struct PendingEnrollment {
    id: String,
    grant_id: String,
    node_uid: String,
    cluster: String,
    hardware: String,
    token: String,
    key: String,
    csr: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Credential {
    pub fingerprint: String,
    pub node_uid: String,
    pub cluster: String,
    pub grant_id: String,
    pub tls: Tls,
    pub token_file: PathBuf,
    pub expires_ms: u64,
}
#[derive(Serialize, Deserialize)]
struct Renewal {
    id: String,
    key: String,
    csr: String,
    #[serde(default)]
    certificate: Option<pki::Signed>,
}
fn hash(value: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(value))
}
fn read_private<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let value = crate::credentials::security::secret(path)?;
    ensure!(
        value.len() <= 1024 * 1024,
        "private identity file too large"
    );
    Ok(serde_json::from_str(&value)?)
}
fn ensure_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    ensure!(
        fs::symlink_metadata(path)?.is_dir(),
        "identity directory must not be a symlink"
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
pub async fn bootstrap(
    config: &Path,
    node_uid: &str,
    output: &Path,
    ttl: u64,
    recover: bool,
) -> Result<()> {
    bootstrap_with_config(
        crate::interface::profile::load(config, None)?,
        node_uid,
        output,
        ttl,
        recover,
    )
    .await
}
pub async fn bootstrap_with_config(
    cfg: crate::interface::client::Config,
    node_uid: &str,
    output: &Path,
    ttl: u64,
    recover: bool,
) -> Result<()> {
    ensure!(
        converyn_core::model::domain::valid_id(node_uid) && (60..=3600).contains(&ttl),
        "invalid bootstrap identity or lifetime"
    );
    ensure!(
        !output.exists(),
        "refusing to replace existing bootstrap credentials"
    );
    let cluster = cfg.cluster.clone();
    let servers = cfg.servers.clone();
    let ca = cfg.ca_pem()?;
    let api = cfg.client()?;
    let parent = output.parent().context("bootstrap output parent missing")?;
    if !parent.exists() {
        ensure_directory(parent)?;
    }
    let pending_path = output.with_extension("pending.json");
    let pending: BootstrapPending = if pending_path.exists() {
        let p: BootstrapPending = read_private(&pending_path)?;
        ensure!(
            p.node_uid == node_uid && p.ttl == ttl && p.recover == recover,
            "bootstrap retry input mismatch"
        );
        p
    } else {
        let (key, csr) = pki::request("bootstrap")?;
        let p = BootstrapPending {
            id: pki::random_token()?,
            node_uid: node_uid.into(),
            token: pki::random_token()?,
            key,
            csr,
            ttl,
            recover,
        };
        pki::private(&pending_path, &serde_json::to_vec(&p)?)?;
        p
    };
    let reply=crate::interface::client::request(&api,reqwest::Method::POST,"/api/v1/identity/bootstrap",Some(&serde_json::json!({"id":pending.id,"node_uid":node_uid,"token_sha256":hash(&pending.token),"csr_pem":pending.csr,"ttl_seconds":ttl,"recover":recover}))).await?;
    let grant: converyn_core::identities::registration::Bootstrap =
        serde_json::from_value(reply["grant"].clone())?;
    ensure!(
        grant.expires_ms > crate::executors::host::now_ms(),
        "bootstrap expired; create a new request rather than extending a consumed identity"
    );
    let bundle = BootstrapBundle {
        cluster,
        servers,
        node_uid: node_uid.into(),
        grant_id: grant.id,
        token: pending.token,
        private_key_pem: pending.key,
        certificate_pem: grant.certificate.certificate_pem,
        ca_pem: ca,
        expires_ms: grant.expires_ms,
    };
    pki::private(output, &serde_json::to_vec_pretty(&bundle)?)?;
    fs::remove_file(pending_path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
fn materialize(path: &Path, key: &str, cert: &str, ca: &str) -> Result<Tls> {
    ensure_directory(path)?;
    let key_path = path.join("tls.key");
    let cert_path = path.join("tls.crt");
    let ca_path = path.join("ca.crt");
    let pair = rcgen::KeyPair::from_pem(key)?;
    ensure!(
        pki::info(cert)?.public_key == pair.public_key_raw(),
        "issued certificate does not match local private key"
    );
    for (path, value) in [(&key_path, key), (&cert_path, cert), (&ca_path, ca)] {
        crate::nodes::agent::durable(path, value.as_bytes())?;
    }
    Ok(Tls {
        key: key_path,
        cert: cert_path,
        ca: ca_path,
    })
}
pub fn active(cfg: &Config) -> Result<Option<Credential>> {
    if !cfg.enrolled {
        return Ok(None);
    }
    let path = cfg.state_dir.join("credential.json");
    if !path.exists() {
        return Ok(None);
    }
    let c: Credential = read_private(&path)?;
    ensure!(
        c.node_uid == cfg.node_uid && c.cluster == cfg.cluster,
        "credential directory identity mismatch"
    );
    Ok(Some(c))
}
pub async fn enroll(
    bootstrap_file: &Path,
    state: &Path,
    output: &Path,
    template: Option<&Path>,
    hardware_file: Option<&Path>,
) -> Result<()> {
    use fs2::FileExt;
    let bundle: BootstrapBundle = read_private(bootstrap_file)?;
    ensure!(
        converyn_core::model::domain::valid_id(&bundle.node_uid)
            && converyn_core::model::domain::valid_id(&bundle.cluster),
        "invalid bootstrap identity"
    );
    ensure!(
        converyn_core::model::domain::valid_id(&bundle.grant_id)
            && bundle.token.len() >= 32
            && bundle.token.len() <= 256
            && bundle.token.bytes().all(|b| b.is_ascii_graphic()),
        "invalid bootstrap grant or token"
    );
    crate::interface::client::validate_servers(&bundle.servers)?;
    let template_config: Option<Config> = template
        .map(|p| -> Result<Config> { Ok(serde_json::from_slice(&fs::read(p)?)?) })
        .transpose()?;
    if let Some(cfg) = &template_config {
        ensure!(
            cfg.node_uid == bundle.node_uid && cfg.cluster == bundle.cluster,
            "template identity differs from bootstrap target"
        );
    }
    ensure_directory(state)?;
    let state = fs::canonicalize(state)?;
    crate::credentials::security::private_state_directory(&state)?;
    if state.join("identity.json").exists() {
        let binding: serde_json::Value =
            serde_json::from_slice(&fs::read(state.join("identity.json"))?)?;
        ensure!(
            binding == serde_json::json!({"node_uid":bundle.node_uid,"cluster":bundle.cluster}),
            "existing execution directory belongs to another identity"
        );
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(state.join("agent.lock"))?;
    lock.try_lock_exclusive()
        .context("stop Agent before enrollment; existing execution state is retained")?;
    let hardware_path = hardware_file.map(Path::to_path_buf).or_else(|| {
        template_config
            .as_ref()
            .and_then(|c| c.hardware_id_file.clone())
    });
    let hardware = crate::executors::host::hardware(hardware_path.as_deref())?.context(
        "no unique hardware identity available; supply an explicit hardware identity file",
    )?;
    let pending_path = state.join(format!("enroll-{}.json", bundle.grant_id));
    let pending: PendingEnrollment = if pending_path.exists() {
        let p: PendingEnrollment = read_private(&pending_path)?;
        ensure!(
            p.grant_id == bundle.grant_id
                && p.node_uid == bundle.node_uid
                && p.cluster == bundle.cluster
                && p.hardware == hardware,
            "enrollment retry identity mismatch"
        );
        p
    } else {
        let (key, csr) = pki::request(&bundle.node_uid)?;
        let p = PendingEnrollment {
            id: pki::random_token()?,
            grant_id: bundle.grant_id.clone(),
            node_uid: bundle.node_uid.clone(),
            cluster: bundle.cluster.clone(),
            hardware: hardware.clone(),
            token: pki::random_token()?,
            key,
            csr,
        };
        pki::private(&pending_path, &serde_json::to_vec(&p)?)?;
        p
    };
    let tls = materialize(
        &state.join("bootstrap").join(&bundle.grant_id),
        &bundle.private_key_pem,
        &bundle.certificate_pem,
        &bundle.ca_pem,
    )?;
    let api = Client {
        preferred: Default::default(),
        fingerprint: Arc::new(RwLock::new(Some(
            pki::info(&bundle.certificate_pem)?.fingerprint,
        ))),
        http: Arc::new(RwLock::new(tls.client()?)),
        servers: bundle.servers.clone(),
        cluster: bundle.cluster.clone(),
        token: bundle.token,
        hardware: Some(hardware),
        session: None,
    };
    let response=crate::interface::client::request(&api,reqwest::Method::POST,"/api/v1/identity/enroll",Some(&serde_json::json!({"request_id":pending.id,"grant_id":pending.grant_id,"node_uid":pending.node_uid,"hardware_sha256":pending.hardware,"token_sha256":hash(&pending.token),"csr_pem":pending.csr}))).await?;
    let signed: pki::Signed = serde_json::from_value(response["certificate"].clone())?;
    let dir = state.join("credentials").join(&pending.id);
    let tls = materialize(&dir, &pending.key, &signed.certificate_pem, &bundle.ca_pem)?;
    let token_file = dir.join("token");
    crate::nodes::agent::durable(&token_file, pending.token.as_bytes())?;
    let credential = Credential {
        fingerprint: signed.fingerprint.clone(),
        node_uid: bundle.node_uid.clone(),
        cluster: bundle.cluster.clone(),
        grant_id: bundle.grant_id,
        tls: tls.clone(),
        token_file: token_file.clone(),
        expires_ms: signed.expires_ms,
    };
    crate::nodes::agent::durable(
        &state.join("credential.json"),
        &serde_json::to_vec(&credential)?,
    )?;
    let mut cfg: Config = if let Some(cfg) = template_config {
        cfg
    } else {
        Config {
            log_max_bytes: crate::executors::log_retention::default_limit(),
            node_uid: bundle.node_uid.clone(),
            cluster: bundle.cluster.clone(),
            servers: bundle.servers.clone(),
            token_file: token_file.clone(),
            tls: tls.clone(),
            state_dir: state.clone(),
            artifact_sources: Default::default(),
            execution: None,
            hardware_id_file: hardware_path.clone(),
            enrolled: true,
        }
    };
    ensure!(
        cfg.node_uid == bundle.node_uid && cfg.cluster == bundle.cluster,
        "template identity differs from bootstrap target"
    );
    cfg.servers = bundle.servers;
    cfg.token_file = token_file;
    cfg.tls = tls;
    cfg.state_dir = state.clone();
    cfg.hardware_id_file = hardware_path;
    cfg.enrolled = true;
    crate::nodes::agent::durable(output, &serde_json::to_vec_pretty(&cfg)?)?;
    fs::remove_file(pending_path)?;
    fs::remove_dir_all(state.join("bootstrap"))?;
    File::open(&state)?.sync_all()?;
    Ok(())
}
pub async fn renew(cfg: &Config, api: &Client, force: bool) -> Result<bool> {
    if !cfg.enrolled {
        return Ok(false);
    }
    use fs2::FileExt;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cfg.state_dir.join("renew.lock"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(false);
    }
    let current = active(cfg)?.context("enrolled credential is missing")?;
    let changed = {
        let fingerprint = api
            .fingerprint
            .read()
            .map_err(|_| anyhow::anyhow!("identity lock poisoned"))?;
        fingerprint.as_deref() != Some(current.fingerprint.as_str())
    };
    if changed {
        *api.http
            .write()
            .map_err(|_| anyhow::anyhow!("HTTP client lock poisoned"))? = current.tls.client()?;
        *api.fingerprint
            .write()
            .map_err(|_| anyhow::anyhow!("identity lock poisoned"))? =
            Some(current.fingerprint.clone());
    }
    let pending_path = cfg.state_dir.join("renewal.json");
    if !pending_path.exists()
        && !force
        && current
            .expires_ms
            .saturating_sub(crate::executors::host::now_ms())
            > 2 * 86400 * 1000
    {
        return Ok(false);
    }
    let mut pending: Renewal = if pending_path.exists() {
        read_private(&pending_path)?
    } else {
        let (key, csr) = pki::request(&cfg.node_uid)?;
        let p = Renewal {
            id: pki::random_token()?,
            key,
            csr,
            certificate: None,
        };
        pki::private(&pending_path, &serde_json::to_vec(&p)?)?;
        p
    };
    if pending.certificate.is_none() {
        let response = crate::interface::client::request(
            api,
            reqwest::Method::POST,
            &format!("/api/v1/agents/{}/renew", cfg.node_uid),
            Some(&serde_json::json!({"request_id":pending.id,"csr_pem":pending.csr})),
        )
        .await?;
        pending.certificate = Some(serde_json::from_value(response["certificate"].clone())?);
        crate::nodes::agent::durable(&pending_path, &serde_json::to_vec(&pending)?)?;
    }
    let signed = pending.certificate.as_ref().unwrap();
    let ca = fs::read_to_string(&current.tls.ca)?;
    let tls = materialize(
        &cfg.state_dir.join("credentials").join(&pending.id),
        &pending.key,
        &signed.certificate_pem,
        &ca,
    )?;
    let http = tls.client()?;
    let candidate = Client {
        preferred: api.preferred.clone(),
        fingerprint: Arc::new(RwLock::new(Some(signed.fingerprint.clone()))),
        http: Arc::new(RwLock::new(http.clone())),
        servers: api.servers.clone(),
        token: api.token.clone(),
        cluster: api.cluster.clone(),
        hardware: api.hardware.clone(),
        session: api.session.clone(),
    };
    crate::interface::client::request(
        &candidate,
        reqwest::Method::POST,
        &format!("/api/v1/agents/{}/renew/activate", cfg.node_uid),
        Some(&serde_json::json!({"request_id":pending.id})),
    )
    .await?;
    let credential = Credential {
        fingerprint: signed.fingerprint.clone(),
        tls,
        expires_ms: signed.expires_ms,
        ..current
    };
    crate::nodes::agent::durable(
        &cfg.state_dir.join("credential.json"),
        &serde_json::to_vec(&credential)?,
    )?;
    *api.http
        .write()
        .map_err(|_| anyhow::anyhow!("HTTP client lock poisoned"))? = http;
    *api.fingerprint
        .write()
        .map_err(|_| anyhow::anyhow!("identity lock poisoned"))? = Some(signed.fingerprint.clone());
    fs::remove_file(pending_path)?;
    File::open(&cfg.state_dir)?.sync_all()?;
    Ok(true)
}
