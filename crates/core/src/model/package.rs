//! 包操作的可序列化描述与回执。
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Manager {
    Rpm,
    Deb,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub before: Option<String>,
    pub after: Option<String>,
    pub transaction_exit: Option<i32>,
    pub diagnostic: String,
}
