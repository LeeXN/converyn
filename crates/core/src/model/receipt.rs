//! 工作回执绑定执行身份、终态与实际状态证据。
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Canceled,
    NeedsAttention,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub outputs: super::outputs::Values,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<Box<crate::execution::Evidence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<Box<crate::task_log::Logs>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Box<crate::native::Evidence>>,
    pub operation_id: String,
    pub script_sha256: String,
    pub timeout_seconds: u64,
    pub outcome: Outcome,
    pub exit_code: Option<i32>,
    pub boot_id: String,
}
