//! HTTP 检查的纯输入验证与输出证据。
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Observation {
    pub status: u16,
    pub body_sha256: String,
    pub bytes: u64,
}
pub fn validate_url(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value)?;
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "health URL must not contain credentials, query or fragment"
    );
    ensure!(
        url.scheme() == "https"
            || (url.scheme() == "http"
                && url.host_str().is_some_and(|h| h
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback()))),
        "health checks require HTTPS except numeric loopback HTTP"
    );
    Ok(url)
}
