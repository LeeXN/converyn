//! 核心只验证基础证据与插件调用绑定；场景证据的内容由插件验证器验证。
use crate::domain::Operation;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileState {
    #[serde(default)]
    pub bytes: u64,
    pub sha256: String,
    pub mode: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<converyn_plugin_api::Observation>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_before: Option<crate::service::State>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_after: Option<crate::service::State>,
    pub before: Option<FileState>,
    pub after: Option<FileState>,
    pub changed: bool,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<crate::package::Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reboot: Option<crate::host::Reboot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<crate::health::Observation>,
}
impl Evidence {
    pub fn empty(operation: &Operation) -> Self {
        Self {
            operation: operation.clone(),
            plugin: None,
            before: None,
            after: None,
            service_before: None,
            service_after: None,
            changed: false,
            error: None,
            artifact_size: None,
            package: None,

            reboot: None,
            health: None,
        }
    }

    pub fn verified(&self) -> bool {
        (matches!((&self.operation, &self.plugin), (Operation::Plugin{call},Some(p)) if p.verified && p.plugin==call.plugin && p.call_sha256==converyn_plugin_api::digest(call))
            || matches!((&self.operation,&self.after),(Operation::File{sha256,mode,..},Some(s)) if sha256==&s.sha256 && mode==&s.mode)
            || matches!((&self.operation,&self.after),(Operation::Binary{artifact,mode,..},Some(s)) if artifact.sha256==s.sha256 && *mode==s.mode && self.artifact_size==Some(artifact.size) && s.bytes==artifact.size)
            || matches!((&self.operation,&self.health),(Operation::HttpCheck{expected_status,body_sha256,..},Some(h)) if h.status==*expected_status && body_sha256.as_ref().is_none_or(|s|s==&h.body_sha256) && h.bytes<=1024*1024)
            || matches!((&self.operation,&self.reboot),(Operation::Reboot{expected_kernel,requested,..},Some(r)) if r.after_boot.as_ref().is_some_and(|after|!*requested||after!=&r.before_boot) && expected_kernel.as_ref().is_none_or(|k|r.kernel.as_ref()==Some(k)))
            || matches!((&self.operation,&self.package),(Operation::Package{version,..},Some(p)) if p.after.as_ref()==Some(version) && p.transaction_exit==Some(0))
            || matches!((&self.operation,&self.service_after),(Operation::Service{running,enabled,executable_sha256,..},Some(s)) if s.matches(*running,*enabled) && executable_sha256.as_ref().is_none_or(|sha|s.executable_sha256.as_ref()==Some(sha))))
            && self.error.is_none()
    }
}
