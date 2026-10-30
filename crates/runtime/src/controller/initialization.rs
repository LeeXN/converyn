//! Portable, private deployment bundles. Every process is the same binary;
//! paths in controller profiles are resolved relative to the copied profile.
use crate::credentials::pki;
use crate::credentials::security::{Auth, Tls};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerProfile {
    #[serde(default)]
    pub output_keyring: Option<PathBuf>,
    #[serde(default)]
    pub log_store: Option<crate::log_store::Config>,
    pub id: u64,
    pub listen: SocketAddr,
    pub cluster: String,
    pub data_dir: PathBuf,
    pub auth_file: PathBuf,
    pub tls: Tls,
    pub issuer: Option<pki::Authority>,
    #[serde(default)]
    pub plugin_connections: BTreeMap<String, crate::plugins::ConnectionGrant>,
}
impl ServerProfile {
    pub fn settings(mut self, base: &Path) -> Result<crate::controller::server::Settings> {
        let resolve = |path: PathBuf| {
            if path.is_absolute() {
                path
            } else {
                base.join(path)
            }
        };
        self.output_keyring = self.output_keyring.map(resolve);
        self.data_dir = resolve(self.data_dir);
        self.auth_file = resolve(self.auth_file);
        self.tls.cert = resolve(self.tls.cert);
        self.tls.key = resolve(self.tls.key);
        self.tls.ca = resolve(self.tls.ca);
        if let Some(issuer) = &mut self.issuer {
            issuer.cert = resolve(issuer.cert.clone());
            issuer.key = resolve(issuer.key.clone());
        }
        if let Some(logs) = &mut self.log_store {
            logs.resolve(base);
        }
        let auth = serde_json::from_str(&crate::credentials::security::secret(&self.auth_file)?)?;
        Ok(crate::controller::server::Settings {
            log_store: self.log_store,
            output_keyring: self.output_keyring,
            id: self.id,
            listen: self.listen,
            cluster: self.cluster,
            token: String::new(),
            data_dir: self.data_dir,
            auth: Some(auth),
            tls: Some(self.tls),
            issuer: self.issuer,
            plugin_connections: self.plugin_connections,
        })
    }
}
pub fn create(
    path: &Path,
    cluster: &str,
    members: &[String],
    project: &str,
    business_cluster: &str,
    targets: &[String],
) -> Result<()> {
    create_with_lifetime(
        path,
        cluster,
        members,
        project,
        business_cluster,
        targets,
        pki::DEFAULT_LEAF_LIFETIME_SECONDS,
    )
}

pub fn create_with_lifetime(
    path: &Path,
    cluster: &str,
    members: &[String],
    project: &str,
    business_cluster: &str,
    targets: &[String],
    leaf_lifetime_seconds: u64,
) -> Result<()> {
    pki::validate_leaf_lifetime(leaf_lifetime_seconds)?;
    ensure!(
        !path.exists(),
        "refusing to overwrite an initialized cluster bundle"
    );
    ensure!(
        [cluster, project, business_cluster]
            .into_iter()
            .all(converyn_core::model::domain::valid_id),
        "invalid cluster or project identity"
    );
    ensure!(
        targets
            .iter()
            .all(|target| converyn_core::model::domain::valid_id(target)),
        "invalid Kubernetes target identity"
    );
    let mut nodes = BTreeMap::new();
    for member in members {
        let (id, addr) = member.split_once('=').context("members use id=IP:port")?;
        let id = id.parse::<u64>()?;
        let addr = addr.parse::<SocketAddr>()?;
        ensure!(
            id > 0 && !addr.ip().is_unspecified() && nodes.insert(id, addr).is_none(),
            "invalid or duplicate controller identity"
        );
    }
    ensure!(
        matches!(nodes.len(), 1 | 3 | 5),
        "initialize one controller or an HA cluster with three or five controllers"
    );
    ensure!(
        nodes.values().collect::<BTreeSet<_>>().len() == nodes.len(),
        "controller addresses must be unique"
    );
    std::fs::create_dir_all(path)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    let path = std::fs::canonicalize(path)?;
    let mut authority = pki::create_authority(
        &path.join("authority"),
        &format!(
            "converyn-{}",
            &converyn_core::model::domain::digest(&cluster)[..16]
        ),
    )?;
    authority.leaf_lifetime_seconds = leaf_lifetime_seconds;
    let admin = authority.issue(&path.join("admin"), "converyn-admin", &[])?;
    let mut fingerprints = BTreeSet::new();
    for (id, addr) in &nodes {
        fingerprints.insert(
            authority
                .issue(
                    &path.join(format!("controller-{id}")),
                    &format!("controller-{id}"),
                    &[addr.ip()],
                )?
                .fingerprint,
        );
    }
    let admin_token = pki::random_token()?;
    let auth = Auth {
        admin: admin_token.clone(),
        peer: pki::random_token()?,
        agents: BTreeMap::new(),
        certificates: BTreeMap::from([
            ("admin".into(), [admin.fingerprint].into_iter().collect()),
            ("peer".into(), fingerprints),
        ]),
    };
    auth.validate()?;
    auth.validate_bindings()?;
    pki::create_client_authority(
        &authority,
        &path.join("user-authority"),
        "converyn-user-issuer",
    )?;
    let node_authority = pki::create_client_authority(
        &authority,
        &path.join("node-authority"),
        "converyn-node-issuer",
    )?;
    let ca = std::fs::read(&authority.cert)?;
    let issuer_chain = std::fs::read(&node_authority.cert)?;
    let key = crate::credentials::security::secret(&node_authority.key)?;
    crate::credentials::outputs::Keyring::generate(&path.join("output-keys.json"), false)?;
    let output_keys = std::fs::read(path.join("output-keys.json"))?;
    for (id, addr) in &nodes {
        let dir = path.join(format!("controller-{id}"));
        pki::private(&dir.join("output-keys.json"), &output_keys)?;
        pki::private(&dir.join("ca.crt"), &ca)?;
        pki::private(&dir.join("issuer.key"), key.as_bytes())?;
        pki::private(&dir.join("issuer.crt"), &issuer_chain)?;
        pki::private(&dir.join("auth.json"), &serde_json::to_vec_pretty(&auth)?)?;
        let profile = ServerProfile {
            output_keyring: Some("output-keys.json".into()),
            log_store: None,
            plugin_connections: Default::default(),
            id: *id,
            listen: *addr,
            cluster: cluster.into(),
            data_dir: "data".into(),
            auth_file: "auth.json".into(),
            tls: Tls {
                cert: "tls.crt".into(),
                key: "tls.key".into(),
                ca: "ca.crt".into(),
            },
            issuer: Some(pki::Authority {
                cert: "issuer.crt".into(),
                key: "issuer.key".into(),
                leaf_lifetime_seconds,
            }),
        };
        pki::private(
            &dir.join("server.json"),
            &serde_json::to_vec_pretty(&profile)?,
        )?;
    }
    let servers = nodes
        .values()
        .map(|addr| format!("https://{addr}"))
        .collect::<Vec<_>>();
    let dir = path.join("admin");
    pki::private(&dir.join("token"), admin_token.as_bytes())?;
    pki::private(&dir.join("ca.crt"), &ca)?;
    crate::interface::profile::write(
        &dir.join("config.json"),
        cluster,
        crate::interface::client::Config {
            servers,
            cluster: cluster.into(),
            token_file: dir.join("token"),
            tls: crate::credentials::security::Tls {
                cert: dir.join("tls.crt"),
                key: dir.join("tls.key"),
                ca: dir.join("ca.crt"),
            },
            embedded: None,
        },
    )?;
    let members = nodes
        .iter()
        .map(|(id, addr)| (id.to_string(), serde_json::json!({"addr":addr.to_string()})))
        .collect::<BTreeMap<_, _>>();
    pki::private(
        &path.join("members.json"),
        &serde_json::to_vec_pretty(&members)?,
    )?;
    pki::private(
        &path.join("bundle.json"),
        &serde_json::to_vec_pretty(
            &serde_json::json!({"cluster":cluster,"members":nodes,"project":project,"business_cluster":business_cluster,"admin_config":path.join("admin/config.json"),"personal_authority":path.join("user-authority"),"workflow_execution_only":true}),
        )?,
    )?;
    Ok(())
}
