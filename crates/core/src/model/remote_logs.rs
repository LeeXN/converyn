use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub chunk_hashes: Vec<String>,
    #[serde(default)]
    pub missing_ranges: Vec<(u64, u64)>,
    pub run_id: String,
    pub work_id: String,
    pub node_uid: String,
    pub grant: String,
    pub store_id: String,
    pub stream: String,
    pub bytes: u64,
    pub sha256: String,
    pub chunks: u64,
    pub truncated: bool,
    pub expires_ms: u64,
}
impl Manifest {
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.run_id, self.work_id, self.stream)
    }
}
