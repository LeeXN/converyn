use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use subtle::ConstantTimeEq;

/// Static transport identities only. People and their privileges live in the
/// replicated personnel registry, not in per-project or CI scope configuration.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Auth {
    pub admin: String,
    pub peer: String,
    #[serde(default)]
    pub agents: BTreeMap<String, String>,
    #[serde(default)]
    pub certificates: BTreeMap<String, BTreeSet<String>>,
}
impl Auth {
    pub fn validate(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        for token in std::iter::once(&self.admin)
            .chain(std::iter::once(&self.peer))
            .chain(self.agents.values())
        {
            ensure!(
                token.len() >= 32 && token.bytes().all(|b| b.is_ascii_graphic()),
                "invalid transport credential"
            );
            ensure!(seen.insert(token), "transport credentials must be distinct");
        }
        ensure!(
            self.agents
                .keys()
                .all(|n| converyn_core::model::domain::valid_id(n)),
            "invalid node identity"
        );
        Ok(())
    }
    pub fn validate_bindings(&self) -> Result<()> {
        let principals = std::iter::once("admin".to_string())
            .chain(std::iter::once("peer".to_string()))
            .chain(self.agents.keys().map(|n| format!("node:{n}")))
            .collect::<BTreeSet<_>>();
        ensure!(
            self.certificates.keys().cloned().collect::<BTreeSet<_>>() == principals,
            "every transport identity requires a certificate binding"
        );
        ensure!(
            self.certificates.values().all(|set| !set.is_empty()
                && set.len() <= 16
                && set
                    .iter()
                    .all(|s| converyn_core::identities::registration::valid_sha(s))),
            "invalid certificate fingerprints"
        );
        Ok(())
    }
    pub fn principal(&self, token: &str) -> Option<String> {
        let eq = |known: &str| bool::from(known.as_bytes().ct_eq(token.as_bytes()));
        if eq(&self.admin) {
            Some("admin".into())
        } else if eq(&self.peer) {
            Some("peer".into())
        } else {
            self.agents
                .iter()
                .find(|(_, t)| eq(t))
                .map(|(id, _)| format!("node:{id}"))
        }
    }
    pub fn bound(&self, token: &str, fingerprint: &str) -> bool {
        self.principal(token)
            .and_then(|p| self.certificates.get(&p))
            .is_some_and(|set| set.contains(fingerprint))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tls {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub ca: PathBuf,
}
impl Tls {
    pub fn client(&self) -> Result<reqwest::Client> {
        Self::client_pem(
            &std::fs::read(&self.cert)?,
            secret(&self.key)?.as_bytes(),
            &std::fs::read(&self.ca)?,
        )
    }
    /// Portable personal credentials remain in memory instead of temporary key files.
    pub fn client_pem(cert: &[u8], key: &[u8], ca: &[u8]) -> Result<reqwest::Client> {
        let mut identity = cert.to_vec();
        identity.extend_from_slice(b"\n");
        identity.extend_from_slice(key);
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .tls_info(true)
            .redirect(reqwest::redirect::Policy::none())
            .tls_built_in_root_certs(false)
            .identity(reqwest::Identity::from_pem(&identity)?)
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(15));
        let roots = reqwest::Certificate::from_pem_bundle(ca)?;
        ensure!(!roots.is_empty(), "client CA certificates missing");
        for cert in roots {
            builder = builder.add_root_certificate(cert);
        }
        Ok(builder.build()?)
    }
    pub fn client_verifier(&self) -> Result<Arc<dyn rustls::server::danger::ClientCertVerifier>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut &*std::fs::read(&self.ca)?) {
            roots.add(cert?)?;
        }
        Ok(rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build()?)
    }
    pub fn server(&self) -> Result<axum_server::tls_rustls::RustlsConfig> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut &*std::fs::read(&self.ca)?) {
            roots.add(cert?)?;
        }
        let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
        let certs = rustls_pemfile::certs(&mut &*std::fs::read(&self.cert)?)
            .collect::<std::io::Result<Vec<_>>>()?;
        let key = rustls_pemfile::private_key(&mut secret(&self.key)?.as_bytes())?
            .context("private key missing")?;
        let mut config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, key)?;
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(axum_server::tls_rustls::RustlsConfig::from_config(
            Arc::new(config),
        ))
    }
}

pub fn secret(path: &Path) -> Result<String> {
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        std::fs::metadata(path)?.permissions().mode() & 0o077 == 0,
        "credential file must be private (0600)"
    );
    Ok(std::fs::read_to_string(path)?.trim().into())
}

pub fn private_state_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid 仅读取有效 UID，不传递指针或修改进程身份。
    let uid = unsafe { libc::geteuid() };
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0,
        "state directory must be private and owned by the supervisor"
    );
    if uid == 0 {
        for ancestor in path.ancestors() {
            let m = std::fs::metadata(ancestor)?;
            ensure!(
                m.uid() == 0 && (m.mode() & 0o022 == 0 || m.mode() & 0o1000 != 0),
                "root state directory has an untrusted writable ancestor"
            );
        }
    }
    Ok(())
}
