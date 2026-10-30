//! 固定摘要制品描述，不包含网络客户端或来源凭据。
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Location {
    pub source: String,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub sha256: String,
    pub size: u64,
    /// Empty locations select the Converyn controller's managed artifact store.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
}
impl ArtifactRef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.size > 0
                && self.size <= crate::domain::MAX_ARTIFACT_SIZE
                && self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid artifact digest or size"
        );
        ensure!(
            self.locations.len() <= 4,
            "configure at most 4 external artifact sources"
        );
        for location in &self.locations {
            ensure!(
                crate::domain::valid_id(&location.source),
                "invalid source name"
            );
            validate_path(&location.path)?;
        }
        Ok(())
    }
}
pub fn validate_path(path: &str) -> Result<()> {
    ensure!(
        path.starts_with('/')
            && path.len() <= 4096
            && !path.contains(['?', '#', '%', '\\'])
            && path.split('/').skip(1).all(|s| !s.is_empty()
                && s != "."
                && s != ".."
                && s.bytes().all(|b| b.is_ascii_graphic())),
        "artifact path must be normalized, unescaped and query-free"
    );
    Ok(())
}
