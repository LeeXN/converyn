//! 任务执行位置与身份的审计证据。
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Process,
    Systemd,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub uid: u32,
    pub gid: u32,
    pub mode: Mode,
    pub unit: Option<String>,
}
