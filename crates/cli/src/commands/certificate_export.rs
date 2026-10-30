//! Public registry export and local private-bundle export have separate sources.
use super::*;
use converyn_runtime::{
    credentials::pki,
    interface::{client::Config, profile::ContextConfig},
};
use serde_json::{Value, json};
use std::{os::unix::fs::DirBuilderExt, path::Path};

pub(super) fn location(source: &ClientSource, config: &Config) -> Result<Value> {
    let file = std::path::absolute(source.path()?)?;
    let credentials = config
        .embedded
        .as_ref()
        .context("personal config must contain embedded credentials")?;
    let info = pki::info(&credentials.certificate_pem)?;
    let profile = converyn_runtime::interface::profile::read_profile(&source.path()?)?;
    let context = profile.select(source.context.as_deref())?.0;
    Ok(
        json!({"config_file":file,"context":context,"storage":"embedded in selected client context","fingerprint":info.fingerprint,"certificate":"credentials.certificate_pem","private_key":"credentials.private_key_pem","ca":"credentials.ca_pem","token":"credentials.token"}),
    )
}
#[derive(clap::Args)]
#[command(group(clap::ArgGroup::new("certificate_source").required(true).args(["id","current","ca","directory"])))]
pub(crate) struct ExportArgs {
    #[command(flatten)]
    pub connection: super::operator::Connection,
    /// Export this registered public certificate as PEM; no private key or token.
    #[arg(long, conflicts_with = "password_file")]
    id: Option<String>,
    /// Export the selected context's public CA certificate bundle as PEM; no private key or token.
    #[arg(long, conflicts_with = "password_file")]
    ca: bool,
    /// Export a PKCS#12 bundle from the selected local --config/--context, including its private key.
    #[arg(long)]
    current: bool,
    /// Export PKCS#12 from the original private bundle's config.json; independent of client contexts.
    #[arg(long, conflicts_with_all = ["config", "context"])]
    directory: Option<PathBuf>,
    /// New output file; existing files and symlinks are never overwritten.
    #[arg(long)]
    output: PathBuf,
    /// Read the PKCS#12 password from a private file; omit for an interactive OpenSSL prompt.
    #[arg(long)]
    password_file: Option<PathBuf>,
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn prepare(credentials: &converyn_runtime::interface::profile::Credentials) -> Result<Temporary> {
    let root = std::env::temp_dir().join(format!(
        "converyn-certificate-export-{}",
        pki::random_token()?
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&root)?;
    let guard = Temporary(root);
    for (name, bytes) in [
        ("tls.crt", &credentials.certificate_pem),
        ("tls.key", &credentials.private_key_pem),
        ("ca.crt", &credentials.ca_pem),
    ] {
        pki::private(&guard.0.join(name), bytes.as_bytes())?;
    }
    Ok(guard)
}
fn pkcs12(directory: &Path, password_file: Option<PathBuf>) -> Result<Vec<u8>> {
    let mut command = std::process::Command::new("openssl");
    command
        .args(["pkcs12", "-export", "-in"])
        .arg(directory.join("tls.crt"))
        .arg("-inkey")
        .arg(directory.join("tls.key"))
        .arg("-certfile")
        .arg(directory.join("ca.crt"))
        .stdin(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    if let Some(path) = password_file {
        let _ = converyn_runtime::credentials::security::secret(&path)?;
        command
            .arg("-passout")
            .arg("stdin")
            .stdin(std::fs::File::open(path)?);
    }
    let result = command
        .output()
        .context("PKCS#12 export requires an existing OpenSSL installation")?;
    ensure!(
        result.status.success(),
        "PKCS#12 export failed; no output published"
    );
    Ok(result.stdout)
}
pub(super) fn current_pkcs12(config: Config, password_file: Option<PathBuf>) -> Result<Vec<u8>> {
    let context = ContextConfig::from_config(config)?;
    let temporary = prepare(&context.credentials)?;
    pkcs12(&temporary.0, password_file)
}
/// Parse every configured CA and reject non-certificate material before public export.
pub(super) fn ca_bundle(config: &Config) -> Result<(String, Vec<Value>)> {
    let pem = config.ca_pem()?;
    let mut certificates = Vec::new();
    for item in rustls_pemfile::read_all(&mut pem.as_bytes()) {
        let rustls_pemfile::Item::X509Certificate(der) = item? else {
            anyhow::bail!("CA export requires public certificates only");
        };
        let info = pki::info_der(der.as_ref())?;
        ensure!(
            info.is_ca,
            "configured CA bundle contains a non-CA certificate"
        );
        certificates.push(json!({"sha256":info.fingerprint,"expires_ms":info.expires_ms}));
    }
    ensure!(!certificates.is_empty(), "configured CA bundle is empty");
    Ok((pem, certificates))
}
pub(super) async fn export(args: ExportArgs) -> Result<()> {
    ensure!(
        std::fs::symlink_metadata(&args.output)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "refusing to replace existing export"
    );
    if args.ca {
        let (pem, certificates) = ca_bundle(&args.connection.config.load()?)?;
        pki::private(&args.output, pem.as_bytes())?;
        return output::print(
            &json!({"output":args.output,"format":"PEM CA certificate bundle","certificates":certificates,"contains_private_key":false}),
        );
    }
    if let Some(id) = args.id {
        ensure!(
            converyn_core::model::domain::valid_id(&id),
            "invalid credential id"
        );
        let api = args.connection.config.load()?.client()?;
        let value = converyn_runtime::interface::client::request(
            &api,
            reqwest::Method::GET,
            &format!("/api/v1/credentials/{id}"),
            None,
        )
        .await?;
        let pem = value["credential"]["certificate_pem"]
            .as_str()
            .context("public certificate not returned by controller")?;
        let info = pki::info(pem)?;
        ensure!(
            value["credential"]["fingerprint"] == info.fingerprint,
            "registered certificate fingerprint mismatch"
        );
        pki::private(&args.output, pem.as_bytes())?;
        return output::print(
            &json!({"output":args.output,"credential_uid":id,"format":"PEM public certificate","fingerprint":info.fingerprint,"contains_private_key":false}),
        );
    }
    let config = if let Some(directory) = args.directory {
        let directory = std::path::absolute(directory)?;
        ensure!(
            directory.canonicalize()? == directory,
            "credential directory must not traverse symlinks"
        );
        converyn_runtime::credentials::security::private_state_directory(&directory)?;
        ClientSource::explicit(directory.join("config.json")).load()?
    } else {
        ensure!(args.current, "choose --id, --ca, --current or --directory");
        args.connection.config.load()?
    };
    let bytes = current_pkcs12(config, args.password_file)?;
    pki::private(&args.output, &bytes)?;
    output::print(&json!({"output":args.output,"format":"PKCS#12","contains_private_key":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ca_export_preserves_all_cas_and_rejects_keys_and_leaf_certificates() {
        let root = tempfile::tempdir().unwrap();
        let ca = pki::create_authority(&root.path().join("authority"), "test-ca").unwrap();
        let second = pki::create_authority(&root.path().join("second"), "second-ca").unwrap();
        let pem = std::fs::read_to_string(ca.cert).unwrap()
            + &std::fs::read_to_string(second.cert).unwrap();
        let path = root.path().join("ca.pem");
        std::fs::write(&path, &pem).unwrap();
        let config:Config=serde_json::from_value(json!({"cluster":"test","servers":["https://127.0.0.1:8443"],"token_file":"unused","tls":{"cert":"unused","key":"unused","ca":path}})).unwrap();
        let (exported, certificates) = ca_bundle(&config).unwrap();
        assert_eq!(exported, pem);
        assert_eq!(certificates.len(), 2);
        let key = std::fs::read_to_string(ca.key).unwrap();
        std::fs::write(&path, pem + &key).unwrap();
        assert!(ca_bundle(&config).is_err());
        let leaf = pki::create_authority(&root.path().join("leaf-authority"), "leaf-ca")
            .unwrap()
            .issue_personal(&root.path().join("person"), "person")
            .unwrap();
        std::fs::write(&path, leaf.certificate_pem).unwrap();
        assert!(ca_bundle(&config).is_err());
    }
}
