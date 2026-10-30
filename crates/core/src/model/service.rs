//! 服务状态证据，不执行 systemctl。
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct State {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_pid: Option<u32>,
    pub load: String,
    pub active: String,
    pub enabled: String,
    pub sub: String,
}
impl State {
    pub fn matches(&self, running: bool, enabled: bool) -> bool {
        self.load == "loaded"
            && (if running {
                self.active == "active"
            } else {
                self.active == "inactive"
            })
            && self.enabled == if enabled { "enabled" } else { "disabled" }
    }
}
