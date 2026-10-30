//! 将入场时的纯契约结果固定到共识命令。重放不再依赖当前安装的插件实现。
use anyhow::{Context, Result, ensure};
use converyn_plugin_api::{self as api, Call, Contracts, Identity, Observation, ScriptOrigin};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", content = "value", rename_all = "snake_case")]
pub enum Outcome {
    Value(Value),
    Error(String),
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Journal(pub BTreeMap<String, Outcome>);
/// 仅在日志提交前使用，不能从状态机重放路径构造。
pub struct Recorder<'a> {
    inner: &'a dyn Contracts,
    entries: RefCell<BTreeMap<String, Outcome>>,
}
impl<'a> Recorder<'a> {
    pub fn new(inner: &'a dyn Contracts) -> Self {
        Self {
            inner,
            entries: RefCell::new(BTreeMap::new()),
        }
    }
    pub fn finish(self) -> Journal {
        Journal(self.entries.into_inner())
    }
    fn record<T: Serialize>(
        &self,
        method: &str,
        args: &impl Serialize,
        result: Result<T>,
    ) -> Result<T> {
        let value = match &result {
            Ok(v) => Outcome::Value(serde_json::to_value(v)?),
            Err(e) => Outcome::Error(e.to_string()),
        };
        self.entries
            .borrow_mut()
            .insert(api::digest(&(method, args)), value);
        result
    }
}
impl Contracts for Recorder<'_> {
    fn validate_target(&self, id: &Identity, t: &Value) -> Result<()> {
        self.record("target", &(id, t), self.inner.validate_target(id, t))
    }
    fn validate_script(&self, o: &ScriptOrigin, s: &str, r: &[String]) -> Result<()> {
        self.record("script", &(o, s, r), self.inner.validate_script(o, s, r))
    }
    fn validate_call(&self, c: &Call) -> Result<()> {
        self.record("call", c, self.inner.validate_call(c))
    }
    fn bind_target(&self, c: &Call, t: &Value) -> Result<Call> {
        self.record("bind", &(c, t), self.inner.bind_target(c, t))
    }
    fn verify_observation(&self, c: &Call, o: &Observation) -> Result<bool> {
        self.record("observation", &(c, o), self.inner.verify_observation(c, o))
    }
}
/// 输入摘要必须精确匹配。缺失的记录代表预演与提交时前置条件已变化，必须拒绝。
impl Journal {
    fn read<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: &impl Serialize,
    ) -> Result<T> {
        match self
            .0
            .get(&api::digest(&(method, args)))
            .context("contract precondition changed; retry the same intent")?
        {
            Outcome::Value(v) => Ok(serde_json::from_value(v.clone())?),
            Outcome::Error(e) => anyhow::bail!("{e}"),
        }
    }
    pub fn validate_size(&self) -> Result<()> {
        ensure!(
            self.0.len() <= 4096 && serde_json::to_vec(self)?.len() <= 2 * 1024 * 1024,
            "contract journal exceeds consensus limit"
        );
        Ok(())
    }
}
impl Contracts for Journal {
    fn validate_target(&self, id: &Identity, t: &Value) -> Result<()> {
        self.read("target", &(id, t))
    }
    fn validate_script(&self, o: &ScriptOrigin, s: &str, r: &[String]) -> Result<()> {
        self.read("script", &(o, s, r))
    }
    fn validate_call(&self, c: &Call) -> Result<()> {
        self.read("call", c)
    }
    fn bind_target(&self, c: &Call, t: &Value) -> Result<Call> {
        self.read("bind", &(c, t))
    }
    fn verify_observation(&self, c: &Call, o: &Observation) -> Result<bool> {
        self.read("observation", &(c, o))
    }
}
