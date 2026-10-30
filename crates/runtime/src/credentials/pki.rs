//! Certificate creation and CSR signing are part of the distribution binary.
//! Requested CSR privileges are discarded: Node issuance is client-auth only.
use anyhow::{Context, Result, ensure};
pub use converyn_core::identities::certificate::{Info, Signed, info, info_der};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SerialNumber,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
pub const DEFAULT_LEAF_LIFETIME_SECONDS: u64 = 30 * 86400;

pub fn validate_leaf_lifetime(seconds: u64) -> Result<()> {
    ensure!(
        (300..=365 * 86400).contains(&seconds),
        "leaf certificate lifetime must be 300..31536000 seconds"
    );
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub cert: PathBuf,
    pub key: PathBuf,
    #[serde(default = "leaf_lifetime")]
    pub leaf_lifetime_seconds: u64,
}
fn leaf_lifetime() -> u64 {
    DEFAULT_LEAF_LIFETIME_SECONDS
}
pub fn random_token() -> Result<String> {
    let mut value = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut value)?;
    Ok(hex::encode(value))
}
fn params(name: &str, lifetime: u64) -> Result<CertificateParams> {
    ensure!(
        converyn_core::model::domain::valid_id(name) && (60..=3650 * 86400).contains(&lifetime),
        "invalid certificate identity or lifetime"
    );
    let mut p = CertificateParams::new(Vec::<String>::new())?;
    // OpenSSL strict validation requires AKI on issuer-signed certificates.
    // Set it centrally so initial leaves, renewed CSRs and intermediates agree.
    p.use_authority_key_identifier_extension = true;
    p.distinguished_name = rcgen::DistinguishedName::new();
    p.distinguished_name.push(DnType::CommonName, name);
    p.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
    p.not_after = time::OffsetDateTime::now_utc() + time::Duration::seconds(lifetime as i64);
    p.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    p.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let mut serial = [0; 20];
    File::open("/dev/urandom")?.read_exact(&mut serial)?;
    serial[0] = (serial[0] & 0x7f) | 1;
    p.serial_number = Some(SerialNumber::from_slice(&serial));
    Ok(p)
}
pub fn private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )?
    .sync_all()?;
    Ok(())
}
fn directory(path: &Path) -> Result<()> {
    ensure!(
        !path.exists(),
        "refusing to replace existing identity directory"
    );
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}
pub fn create_authority(path: &Path, name: &str) -> Result<Authority> {
    directory(path)?;
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let mut p = params(name, 3650 * 86400)?;
    p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    p.extended_key_usages.clear();
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let cert = p.self_signed(&key)?;
    private(&path.join("ca.key"), key.serialize_pem().as_bytes())?;
    private(&path.join("ca.crt"), cert.pem().as_bytes())?;
    Ok(Authority {
        cert: path.join("ca.crt"),
        key: path.join("ca.key"),
        leaf_lifetime_seconds: leaf_lifetime(),
    })
}
pub fn request(name: &str) -> Result<(String, String)> {
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let mut request = params(name, leaf_lifetime())?;
    request.serial_number = None;
    request.use_authority_key_identifier_extension = false;
    let csr = request.serialize_request(&key)?.pem()?;
    Ok((key.serialize_pem(), csr))
}
impl Authority {
    fn signer(&self) -> Result<(Issuer<'static, KeyPair>, String, Info)> {
        let pem = std::fs::read_to_string(&self.cert)?;
        let info = info(&pem)?;
        let key = KeyPair::from_pem(&crate::credentials::security::secret(&self.key)?)?;
        ensure!(
            info.is_ca
                && info.can_sign
                && info.public_key == key.public_key_raw()
                && info.expires_ms > crate::executors::host::now_ms() + 60000
                && i128::from(info.not_before_ms) <= i128::from(crate::executors::host::now_ms()),
            "issuer is expired, not a signing CA, or does not match its private key"
        );
        Ok((Issuer::from_ca_cert_pem(&pem, key)?, pem, info))
    }
    pub fn validate(&self) -> Result<()> {
        validate_leaf_lifetime(self.leaf_lifetime_seconds)?;
        self.signer()?;
        Ok(())
    }
    pub fn sign_client(&self, name: &str, csr_pem: &str, lifetime: u64) -> Result<Signed> {
        ensure!(csr_pem.len() <= 16384, "CSR exceeds 16 KiB");
        let (issuer, chain, ca) = self.signer()?;
        let mut csr = CertificateSigningRequestParams::from_pem(csr_pem)?;
        csr.params = params(name, lifetime.min(self.leaf_lifetime_seconds))?;
        csr.params.not_after = csr
            .params
            .not_after
            .min(time::OffsetDateTime::from_unix_timestamp(
                (ca.expires_ms / 1000) as i64,
            )?);
        // The public key is verified by the CSR parser; its requested CA, SAN,
        // server-auth and name extensions are intentionally not copied.
        let cert = csr.signed_by(&issuer)?;
        let pem = cert.pem();
        let details = info(&pem)?;
        Ok(Signed {
            certificate_pem: format!("{pem}{chain}"),
            fingerprint: details.fingerprint,
            expires_ms: details.expires_ms,
        })
    }
    pub fn issue(&self, path: &Path, name: &str, addresses: &[std::net::IpAddr]) -> Result<Signed> {
        validate_leaf_lifetime(self.leaf_lifetime_seconds)?;
        self.issue_inner(path, name, addresses)
    }
    pub fn issue_personal(&self, path: &Path, name: &str) -> Result<Signed> {
        ensure!(
            (300..=converyn_core::identities::personnel::MAX_LIFETIME)
                .contains(&self.leaf_lifetime_seconds),
            "personal lifetime must be 300..94608000 seconds"
        );
        let (_, _, ca) = self.signer()?;
        ensure!(
            crate::executors::host::now_ms().saturating_add(self.leaf_lifetime_seconds * 1000)
                <= ca.expires_ms,
            "requested lifetime exceeds issuer validity"
        );
        self.issue_inner(path, name, &[])
    }
    fn issue_inner(
        &self,
        path: &Path,
        name: &str,
        addresses: &[std::net::IpAddr],
    ) -> Result<Signed> {
        directory(path)?;
        let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let (issuer, chain, ca) = self.signer()?;
        let mut p = params(name, self.leaf_lifetime_seconds)?;
        p.not_after = p.not_after.min(time::OffsetDateTime::from_unix_timestamp(
            (ca.expires_ms / 1000) as i64,
        )?);
        if !addresses.is_empty() {
            p.extended_key_usages
                .push(ExtendedKeyUsagePurpose::ServerAuth);
            p.subject_alt_names = addresses
                .iter()
                .copied()
                .map(rcgen::SanType::IpAddress)
                .collect();
        }
        let cert = p.signed_by(&key, &issuer)?;
        let pem = cert.pem();
        let details = info(&pem)?;
        let certificate_pem = format!("{pem}{chain}");
        private(&path.join("tls.key"), key.serialize_pem().as_bytes())?;
        private(&path.join("tls.crt"), certificate_pem.as_bytes())?;
        Ok(Signed {
            certificate_pem,
            fingerprint: details.fingerprint,
            expires_ms: details.expires_ms,
        })
    }
}

/// Online controllers receive a client-only intermediate. The transport root
/// key remains on the operator machine and cannot mint server leaves remotely.
pub fn create_client_authority(root: &Authority, path: &Path, name: &str) -> Result<Authority> {
    directory(path)?;
    let (issuer, chain, root_info) = root.signer()?;
    let key = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let mut p = params(name, 5 * 365 * 86400)?;
    p.not_after = p.not_after.min(time::OffsetDateTime::from_unix_timestamp(
        (root_info.expires_ms / 1000) as i64,
    )?);
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let cert = p.signed_by(&key, &issuer)?;
    private(&path.join("ca.key"), key.serialize_pem().as_bytes())?;
    private(
        &path.join("ca.crt"),
        format!("{}{chain}", cert.pem()).as_bytes(),
    )?;
    Ok(Authority {
        cert: path.join("ca.crt"),
        key: path.join("ca.key"),
        leaf_lifetime_seconds: leaf_lifetime(),
    })
}

pub fn serial(pem: &str) -> Result<String> {
    use x509_parser::prelude::FromDer;
    let leaf = rustls_pemfile::certs(&mut pem.as_bytes())
        .next()
        .context("missing certificate")??;
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(leaf.as_ref())
        .map_err(|_| anyhow::anyhow!("invalid certificate"))?;
    Ok(cert.raw_serial_as_string())
}
