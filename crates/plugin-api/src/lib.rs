//! 编译期插件的最小契约。此 crate 不依赖编排器、HTTP、数据库或共识实现。
//! 插件必须输出可固定的声明与有界证据；宿主负责授权、日志、持久回执和调度。
pub mod local;
pub mod variables;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, sync::atomic::AtomicBool};

/// 插件执行位置。控制端工作不冒充拥有 Node 证书的 Agent。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    Node,
    Controller,
}

/// 固定到实现内容的身份，版本相同但内容不同也不能替代已批准的插件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub version: String,
    pub contract: u32,
    pub sha256: String,
}

/// 计划内的插件调用。资源与收敛标志由插件生成，宿主在入库前重新验证。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub plugin: Identity,
    pub action: String,
    pub location: Location,
    pub spec: Value,
    pub resources: Vec<String>,
    pub reconcilable: bool,
    pub timeout_seconds: u32,
}
impl Call {
    /// 只验证通用编码约束，不把合法 JSON 误认为插件已经支持该动作。
    pub fn validate(&self) -> Result<()> {
        ensure!(
            valid_id(&self.plugin.id)
                && valid_id(&self.plugin.version)
                && self.plugin.contract == 1
                && valid_sha(&self.plugin.sha256),
            "invalid plugin identity"
        );
        ensure!(
            valid_id(&self.action)
                && (1..=7200).contains(&self.timeout_seconds)
                && self.resources.len() <= 128,
            "invalid plugin action or limits"
        );
        ensure!(
            self.resources.windows(2).all(|w| w[0] < w[1])
                && self.resources.iter().all(|r| !r.is_empty()
                    && r.len() <= 4096
                    && !r.contains(['\0', '\n'])
                    && !r.split('/').any(|s| s == "..")),
            "invalid or noncanonical plugin resources"
        );
        ensure!(
            serde_json::to_vec(&self.spec)?.len() <= 512 * 1024,
            "plugin specification too large"
        );
        Ok(())
    }
}

/// 运行前已经冻结的能力声明，也是同一控制面的兼容性比较依据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Descriptor {
    pub identity: Identity,
    pub location: Location,
    pub actions: Vec<String>,
    pub description: String,
}

/// 与调用摘要绑定的证据。verified 只由宿主调用插件验证器后设置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub owner: String,
    pub fence: u64,
    pub plugin: Identity,
    pub call_sha256: String,
    pub payload: Value,
    pub verified: bool,
}

/// 原生插件返回实际观察，不直接构造或写入编排器的状态。
pub struct Applied {
    pub changed: bool,
    pub payload: Value,
}

/// 插件可调用的宿主基础能力。实现须继续执行路径和凭据策略。
pub trait Host: Send + Sync {
    fn resolve(&self, values: &variables::Variables) -> Result<variables::Resolved>;
    fn secret(&self, path: &Path) -> Result<String>;
    fn private_directory(&self, path: &Path) -> Result<()>;
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    fn ensure_file(&self, path: &Path, bytes: &[u8], mode: u32) -> Result<bool>;
}

/// 只在运行时存在；本地凭据和取消信号不会被序列化进 Plan。
pub struct Context<'a> {
    pub project: &'a str,
    pub owner: &'a str,
    pub state_dir: &'a Path,
    pub policy: &'a Value,
    pub target: Option<&'a Value>,
    pub fence: u64,
    pub cancel: &'a AtomicBool,
    pub host: &'a dyn Host,
}

/// 静态插件契约。validate/verify 必须是纯函数，不能读取时钟、网络和本地配置。
pub trait Plugin: Send + Sync {
    /// 只接受公开目标身份，拒绝把连接凭据误写入复制状态。
    fn validate_target(&self, _target: &Value) -> Result<()> {
        anyhow::bail!("plugin does not manage external targets")
    }
    /// 声明本地配置引用的读写路径，宿主继续保护自身状态和凭据。
    fn policy_paths(&self, _policy: &Value) -> Result<PolicyPaths> {
        Ok(PolicyPaths::default())
    }
    fn descriptor(&self) -> Descriptor;
    fn compile(&self, action: &str, spec: Value) -> Result<Call>;
    fn validate(&self, call: &Call) -> Result<()> {
        call.validate()?;
        ensure!(
            *call == self.compile(&call.action, call.spec.clone())?,
            "plugin call differs from canonical contract"
        );
        Ok(())
    }
    /// 执行修改并返回独立观察；任何中断由宿主保留为未知结果。
    /// 本地策略的无副作用预检；宿主在领取执行授权前调用。
    fn authorize(&self, call: &Call, _policy: &Value, _project: &str) -> Result<()> {
        self.validate(call)
    }
    fn apply(&self, call: &Call, context: &Context<'_>) -> Result<Applied>;
    /// 仅读取实际状态，用于持续检查；不具备此能力的插件返回错误。
    fn observe(&self, _call: &Call, _context: &Context<'_>) -> Result<Value> {
        anyhow::bail!("plugin does not support observation")
    }
    fn prepare(
        &self,
        _action: &str,
        _input: Value,
        _directory: &Path,
        _host: &dyn PreparationHost,
    ) -> Result<()> {
        anyhow::bail!("plugin has no local preparation command")
    }
    fn expand(&self, _action: &str, _input: Value) -> Result<Expansion> {
        anyhow::bail!("plugin is not a Playbook package")
    }
    fn script_resources(&self, _action: &str) -> Vec<String> {
        Vec::new()
    }
    fn contains_script(&self, _action: &str, _sha256: &str) -> bool {
        false
    }
    fn verify_bound(
        &self,
        call: &Call,
        payload: &Value,
        _owner: &str,
        _fence: u64,
    ) -> Result<bool> {
        self.verify(call, payload)
    }
    fn bind_target(&self, call: &Call, target: &Value) -> Result<Call> {
        self.validate(call)?;
        ensure!(
            self.target_binding(call).as_ref() == Some(target),
            "plugin does not support rebinding this target"
        );
        Ok(call.clone())
    }
    fn target_binding(&self, _call: &Call) -> Option<Value> {
        None
    }
    fn verify(&self, call: &Call, payload: &Value) -> Result<bool>;
}

/// 显式装配的注册表；相同 ID 不能被后注册插件覆盖。
#[derive(Default)]
pub struct Registry {
    plugins: std::collections::BTreeMap<String, Box<dyn Plugin>>,
}
impl Registry {
    pub fn register(&mut self, plugin: impl Plugin + 'static) -> Result<()> {
        let d = plugin.descriptor();
        ensure!(
            valid_id(&d.identity.id)
                && valid_id(&d.identity.version)
                && valid_sha(&d.identity.sha256)
                && d.identity.contract == 1,
            "invalid plugin descriptor"
        );
        ensure!(
            !self.plugins.contains_key(&d.identity.id),
            "duplicate plugin ID"
        );
        self.plugins.insert(d.identity.id, Box::new(plugin));
        Ok(())
    }
    pub fn descriptors(&self) -> Vec<Descriptor> {
        self.plugins.values().map(|p| p.descriptor()).collect()
    }
    pub fn fingerprint(&self) -> String {
        digest(&self.descriptors())
    }
    pub fn get(&self, id: &str) -> Result<&dyn Plugin> {
        self.plugins
            .get(id)
            .map(Box::as_ref)
            .ok_or_else(|| anyhow::anyhow!("plugin {id} is not compiled into this binary"))
    }
    pub fn validate(&self, call: &Call) -> Result<()> {
        self.get(&call.plugin.id)?.validate(call)
    }
    pub fn verify(&self, call: &Call, observation: &Observation) -> Result<bool> {
        self.validate(call)?;
        Ok(observation.plugin == call.plugin
            && observation.call_sha256 == digest(call)
            && observation.verified
            && self.get(&call.plugin.id)?.verify_bound(
                call,
                &observation.payload,
                &observation.owner,
                observation.fence,
            )?)
    }
}
pub fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && !s.contains("..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub fn digest(value: &impl Serialize) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("serializable protocol value"),
    ))
}

/// 展开后的脚本仍记录插件来源。执行配置名称仅作引用，不携带权限参数。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScriptOrigin {
    pub plugin: Identity,
    pub action: String,
}

/// 构建时嵌入的脚本包展开结果，审批必须覆盖全部步骤和定义摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expansion {
    pub plugin: Identity,
    pub action: String,
    pub steps: Vec<Value>,
    pub definitions: std::collections::BTreeMap<String, String>,
}

/// 本地配置准备使用宿主已有 PKI，插件无需依赖运行时 crate 或另一套密码库。
pub struct PreparedAuthority {
    pub cert: std::path::PathBuf,
    pub key: std::path::PathBuf,
}
pub struct CertificateSummary {
    pub is_ca: bool,
    pub can_sign: bool,
    pub expires_ms: u64,
    pub discovery_pin: String,
}
pub trait PreparationHost {
    fn create_authority(&self, directory: &Path, name: &str) -> Result<PreparedAuthority>;
    fn describe_certificate(&self, pem: &[u8]) -> Result<CertificateSummary>;
    fn random_token(&self) -> Result<String>;
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    fn now_ms(&self) -> u64;
}

/// 状态机仅接触此纯契约视图，不能取得执行上下文、凭据或外部客户端。
pub trait Contracts {
    fn validate_target(&self, _identity: &Identity, _target: &Value) -> Result<()> {
        anyhow::bail!("external target contract not supplied")
    }
    fn validate_script(
        &self,
        _origin: &ScriptOrigin,
        _sha256: &str,
        _resources: &[String],
    ) -> Result<()> {
        anyhow::bail!("script package contract not supplied")
    }
    fn validate_call(&self, call: &Call) -> Result<()>;
    fn bind_target(&self, call: &Call, target: &Value) -> Result<Call>;
    fn verify_observation(&self, call: &Call, observation: &Observation) -> Result<bool>;
}
impl Contracts for Registry {
    fn validate_target(&self, identity: &Identity, target: &Value) -> Result<()> {
        let plugin = self.get(&identity.id)?;
        let d = plugin.descriptor();
        ensure!(
            d.identity == *identity && d.location == Location::Controller,
            "external target contract mismatch"
        );
        plugin.validate_target(target)
    }
    fn validate_script(
        &self,
        origin: &ScriptOrigin,
        sha256: &str,
        resources: &[String],
    ) -> Result<()> {
        let plugin = self.get(&origin.plugin.id)?;
        ensure!(
            plugin.descriptor().identity == origin.plugin
                && plugin.contains_script(&origin.action, sha256)
                && plugin.script_resources(&origin.action) == resources,
            "script differs from compiled package"
        );
        Ok(())
    }
    fn validate_call(&self, call: &Call) -> Result<()> {
        self.validate(call)
    }
    fn bind_target(&self, call: &Call, target: &Value) -> Result<Call> {
        self.get(&call.plugin.id)?.bind_target(call, target)
    }
    fn verify_observation(&self, call: &Call, e: &Observation) -> Result<bool> {
        self.verify(call, e)
    }
}
/// 核心单独使用时只能执行基础操作，不能假设未知插件存在。
pub struct NoPlugins;
impl Contracts for NoPlugins {
    fn validate_call(&self, _: &Call) -> Result<()> {
        anyhow::bail!("plugin contract not supplied")
    }
    fn bind_target(&self, _: &Call, _: &Value) -> Result<Call> {
        anyhow::bail!("plugin contract not supplied")
    }
    fn verify_observation(&self, _: &Call, _: &Observation) -> Result<bool> {
        Ok(false)
    }
}

/// 本地策略路径声明，不属于远端权限申请。
#[derive(Default)]
pub struct PolicyPaths {
    pub writable: Vec<std::path::PathBuf>,
    pub protected: Vec<std::path::PathBuf>,
}

/// Registered workflows share a single internal namespace; this is not a user role.
pub fn default_projects() -> std::collections::BTreeSet<String> {
    std::collections::BTreeSet::from(["default".to_string()])
}

/// Workflow execution has one fixed internal namespace, not a user project.
pub fn default_namespace() -> String {
    "default".to_string()
}
