//! 重启证据以 boot ID 为依据，不使用 revision 判断完成。
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reboot {
    pub before_boot: String,
    pub after_boot: Option<String>,
    pub kernel: Option<String>,
    pub requested_ms: u64,
}
