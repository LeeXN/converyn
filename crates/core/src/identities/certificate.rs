//! 纯证书解析，仅核对复制记录中的身份，不签发或读取私钥。
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x509_parser::prelude::FromDer;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signed {
    pub certificate_pem: String,
    pub fingerprint: String,
    pub expires_ms: u64,
}
pub struct Info {
    pub fingerprint: String,
    pub expires_ms: u64,
    pub not_before_ms: i64,
    pub is_ca: bool,
    pub can_sign: bool,
    pub public_key: Vec<u8>,
    pub client_auth: bool,
    pub server_auth: bool,
}
pub fn info(pem: &str) -> Result<Info> {
    let der = rustls_pemfile::certs(&mut pem.as_bytes())
        .next()
        .context("certificate missing")??;
    info_der(der.as_ref())
}
pub fn info_der(der: &[u8]) -> Result<Info> {
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(der)
        .map_err(|_| anyhow::anyhow!("invalid certificate"))?;
    let usages = cert
        .extended_key_usage()
        .map_err(|_| anyhow::anyhow!("invalid certificate usages"))?;
    Ok(Info {
        fingerprint: hex::encode(Sha256::digest(der)),
        expires_ms: u64::try_from(cert.validity().not_after.timestamp())? * 1000,
        not_before_ms: cert.validity().not_before.timestamp() * 1000,
        is_ca: cert
            .basic_constraints()
            .map_err(|_| anyhow::anyhow!("invalid CA constraints"))?
            .is_some_and(|b| b.value.ca),
        can_sign: cert
            .key_usage()
            .map_err(|_| anyhow::anyhow!("invalid key usage"))?
            .is_some_and(|k| k.value.key_cert_sign()),
        public_key: cert.public_key().subject_public_key.data.to_vec(),
        client_auth: usages.as_ref().is_some_and(|u| u.value.client_auth),
        server_auth: usages.as_ref().is_some_and(|u| u.value.server_auth),
    })
}
