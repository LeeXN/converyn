//! 有界日志摘要，原始日志由运行时保管。
use serde::{Deserialize, Serialize};
pub const LOG_LIMIT: u64 = 8 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stream {
    pub stored_bytes: u64,
    pub seen_bytes: u64,
    pub sha256: String,
    pub tail: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Logs {
    pub stdout: Stream,
    pub stderr: Stream,
}
impl Logs {
    pub fn valid(&self) -> bool {
        [&self.stdout, &self.stderr].into_iter().all(|s| {
            s.stored_bytes <= LOG_LIMIT
                && s.seen_bytes >= s.stored_bytes
                && s.tail.len() <= 4096
                && s.sha256.len() == 64
                && s.sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    }
}
