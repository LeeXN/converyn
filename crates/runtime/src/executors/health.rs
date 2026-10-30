//! Read-only application readiness probes. No control-plane credential or client
//! certificate is reused for an application URL; local policy pins the origin.
use crate::executors::native::{Evidence, Policy};
use anyhow::{Result, ensure};
use converyn_core::model::domain::Operation;
pub use converyn_core::model::health::{Observation, validate_url};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub fn check(policy: &Policy, operation: &Operation, cancel: &AtomicBool) -> Result<Evidence> {
    let Operation::HttpCheck {
        url,
        expected_status,
        body_sha256,
        timeout_seconds,
    } = operation
    else {
        anyhow::bail!("not an HTTP probe")
    };
    let url = validate_url(url)?;
    ensure!(
        policy
            .health_origins
            .contains(&url.origin().ascii_serialization()),
        "health origin is not allowed"
    );
    let deadline = Instant::now() + Duration::from_secs(u64::from(*timeout_seconds));
    let mut last = "health endpoint unavailable".to_string();
    loop {
        ensure!(!cancel.load(Ordering::SeqCst), "health probe canceled");
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "{last}");
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(remaining.min(Duration::from_secs(3)))
            .timeout(remaining.min(Duration::from_secs(10)))
            .build()?;
        match client.get(url.clone()).send() {
            Ok(response) => {
                let status = response.status().as_u16();
                let mut bytes = Vec::new();
                let read = response.take(1024 * 1024 + 1).read_to_end(&mut bytes);
                ensure!(bytes.len() <= 1024 * 1024, "health response exceeds 1 MiB");
                if read.is_ok() {
                    let hash = hex::encode(Sha256::digest(&bytes));
                    if status == *expected_status
                        && body_sha256.as_ref().is_none_or(|wanted| wanted == &hash)
                    {
                        let mut evidence = Evidence::empty(operation);
                        evidence.health = Some(Observation {
                            status,
                            body_sha256: hash,
                            bytes: bytes.len() as u64,
                        });
                        return Ok(evidence);
                    }
                    last = format!("health status/body mismatch (HTTP {status})");
                } else {
                    last = "health response incomplete".into();
                }
            }
            Err(_) => last = "health endpoint unavailable or timed out".into(),
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(500)),
        );
    }
}
