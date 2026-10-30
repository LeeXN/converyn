//! Docker 场景插件：保留实际 Engine 状态与配置核对，不访问编排器内部。
pub mod container;
use anyhow::{Result, ensure};
use converyn_plugin_api::{self as api, Plugin};
use serde_json::Value;
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
pub struct Docker;
impl Plugin for Docker {
    fn policy_paths(&self, p: &Value) -> Result<api::PolicyPaths> {
        let rules: container::Rules = serde_json::from_value(p.clone())?;
        Ok(api::PolicyPaths {
            writable: rules.volume_roots,
            protected: rules
                .credentials
                .values()
                .map(|c| c.directory.clone())
                .collect(),
        })
    }
    fn descriptor(&self) -> api::Descriptor {
        api::Descriptor {
            identity: api::Identity {
                id: "converyn.docker".into(),
                version: "1.0.0".into(),
                contract: 1,
                sha256: CONTENT_SHA256.into(),
            },
            location: api::Location::Node,
            actions: vec!["ensure_container".into()],
            description: "Docker container deployment and observed health".into(),
        }
    }
    fn compile(&self, action: &str, spec: Value) -> Result<api::Call> {
        ensure!(action == "ensure_container", "unknown Docker action");
        let s: container::Spec = serde_json::from_value(spec.clone())?;
        s.validate()?;
        Ok(api::Call {
            plugin: self.descriptor().identity,
            action: action.into(),
            location: api::Location::Node,
            spec,
            resources: vec![format!("container/{}", s.name)],
            reconcilable: true,
            timeout_seconds: s.readiness_timeout_seconds.max(60),
        })
    }
    fn authorize(&self, call: &api::Call, policy: &Value, _project: &str) -> Result<()> {
        self.validate(call)?;
        let s: container::Spec = serde_json::from_value(call.spec.clone())?;
        let rules: container::Rules = serde_json::from_value(policy.clone())?;
        let (_, registry, _) = container::normalized_image(&s.image)?;
        ensure!(
            rules.names.contains(&s.name)
                && rules.registries.contains(&registry)
                && (s.user_id > 0 || rules.allow_root_user)
                && s.mounts.iter().all(|m| rules
                    .volume_roots
                    .contains(&std::path::PathBuf::from(&m.source))),
            "Docker name/registry/user/mount not granted locally"
        );
        Ok(())
    }
    fn apply(&self, call: &api::Call, context: &api::Context<'_>) -> Result<api::Applied> {
        self.validate(call)?;
        container::reconcile(context, &serde_json::from_value(call.spec.clone())?)
    }
    fn verify_bound(
        &self,
        call: &api::Call,
        payload: &Value,
        owner: &str,
        _fence: u64,
    ) -> Result<bool> {
        Ok(payload["owner"] == owner && self.verify(call, payload)?)
    }
    fn verify(&self, call: &api::Call, payload: &Value) -> Result<bool> {
        let s: container::Spec = serde_json::from_value(call.spec.clone())?;
        let o: container::Observation = serde_json::from_value(payload.clone())?;
        Ok(o.healthy
            && o.platform == s.platform
            && s.configuration_sha256()? == o.configuration_sha256
            && s.image.ends_with(&format!("@sha256:{}", o.image_digest))
            && !o.container_id.is_empty()
            && !o.image_id.is_empty())
    }
}
