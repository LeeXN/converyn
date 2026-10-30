//! 独立插件示例：通过宿主私有文件能力保存值；无需新增核心操作或数据库表。
//! changes 是此示例自身的观察数据，不被用作编排任务的完成判断。
use anyhow::{Result, ensure};
use converyn_plugin_api::{self as api, Plugin};
use serde::{Deserialize, Serialize};
use serde_json::Value;
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub value: String,
    pub changes: u64,
}
pub struct Example;
impl Plugin for Example {
    fn descriptor(&self) -> api::Descriptor {
        api::Descriptor {
            identity: api::Identity {
                id: "example.value".into(),
                version: "1.0.0".into(),
                contract: 1,
                sha256: CONTENT_SHA256.into(),
            },
            location: api::Location::Node,
            actions: vec!["ensure".into()],
            description: "Minimal independent SDK example; excluded from full production build"
                .into(),
        }
    }
    fn compile(&self, action: &str, spec: Value) -> Result<api::Call> {
        let s: Spec = serde_json::from_value(spec.clone())?;
        ensure!(
            action == "ensure" && api::valid_id(&s.name) && s.value.len() <= 1024,
            "invalid example specification"
        );
        Ok(api::Call {
            plugin: self.descriptor().identity,
            action: action.into(),
            location: api::Location::Node,
            spec,
            resources: vec![format!("example/{}", s.name)],
            reconcilable: true,
            timeout_seconds: 30,
        })
    }
    fn apply(&self, call: &api::Call, c: &api::Context<'_>) -> Result<api::Applied> {
        self.validate(call)?;
        let s: Spec = serde_json::from_value(call.spec.clone())?;
        let path = c.state_dir.join(format!("example-{}.json", s.name));
        let old = c
            .host
            .secret(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<State>(&s).ok());
        let changed = old.as_ref().is_none_or(|p| p.value != s.value);
        let state = State {
            value: s.value,
            changes: old.map_or(0, |s| s.changes) + u64::from(changed),
        };
        let payload = serde_json::to_value(state)?;
        if changed {
            c.host
                .write_private(&path, &serde_json::to_vec(&payload)?)?;
        }
        Ok(api::Applied { changed, payload })
    }
    fn observe(&self, call: &api::Call, c: &api::Context<'_>) -> Result<Value> {
        self.validate(call)?;
        let s: Spec = serde_json::from_value(call.spec.clone())?;
        Ok(serde_json::from_str(&c.host.secret(
            &c.state_dir.join(format!("example-{}.json", s.name)),
        )?)?)
    }
    fn verify(&self, call: &api::Call, payload: &Value) -> Result<bool> {
        let s: Spec = serde_json::from_value(call.spec.clone())?;
        let actual: State = serde_json::from_value(payload.clone())?;
        Ok(actual.value == s.value && actual.changes > 0)
    }
}
