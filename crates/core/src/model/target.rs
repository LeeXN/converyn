//! 外部目标是经过登记的不可变身份快照，不是一个伪装成机器的 Agent。
use super::domain::Node;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
/// plugin 指定目标的身份验证器，spec 不包含凭据，仅包含本地连接引用和公开身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExternalTarget {
    pub uid: String,
    pub revision: u64,
    #[serde(default = "converyn_plugin_api::default_namespace")]
    pub project: String,
    pub plugin: converyn_plugin_api::Identity,
    pub connection: String,
    pub spec: serde_json::Value,
}
impl ExternalTarget {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            converyn_plugin_api::valid_id(&self.uid)
                && converyn_plugin_api::valid_id(&self.connection)
                && self.revision == 1
                && converyn_plugin_api::valid_id(&self.project)
                && converyn_plugin_api::valid_id(&self.plugin.id)
                && converyn_plugin_api::valid_sha(&self.plugin.sha256)
                && serde_json::to_vec(&self.spec)?.len() <= 64 * 1024,
            "invalid external target"
        );
        Ok(())
    }
    /// Controller plugins route through a distinct executor, never a host snapshot.
    pub fn executor(&self) -> Node {
        Node {
            uid: format!(
                "controller-{}",
                &converyn_plugin_api::digest(&self.uid)[..24]
            ),
        }
    }
}
