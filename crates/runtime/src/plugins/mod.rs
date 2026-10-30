//! 静态插件装配与运行时授权。核心状态机只接收已经验证、固定的调用和证据。
use crate::executors::native::{Evidence, Policy};
use anyhow::{Context as _, Result, ensure};
use converyn_core::model::domain::Operation;
use converyn_plugin_api::{self as api, Location, Registry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{OnceLock, atomic::AtomicBool},
};

/// Node 本地授权。插件描述不授予权限，只有受保护配置中的 grant 才授予权限。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub identity: api::Identity,
    #[serde(default = "api::default_projects")]
    pub projects: BTreeSet<String>,
    pub actions: BTreeSet<String>,
    #[serde(default)]
    pub policy: Value,
    #[serde(default)]
    pub protected_paths: Vec<PathBuf>,
}

/// 所有依赖只在这一处装配；插件不依赖本 crate。
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        #[allow(unused_mut)]
        let mut r = Registry::default();
        #[cfg(feature = "plugin-docker")]
        r.register(converyn_plugin_docker::Docker)
            .expect("unique Docker plugin");
        #[cfg(feature = "plugin-kubernetes")]
        r.register(converyn_plugin_kubernetes::Kubernetes)
            .expect("unique Kubernetes plugin");
        #[cfg(feature = "plugin-linux-host")]
        r.register(converyn_plugin_linux_host::LinuxHost)
            .expect("unique Linux host plugin");
        #[cfg(feature = "plugin-kubeadm")]
        r.register(converyn_plugin_kubeadm::Kubeadm)
            .expect("unique kubeadm package");
        #[cfg(feature = "plugin-example")]
        r.register(converyn_plugin_example::Example)
            .expect("unique SDK example");
        r
    })
}

pub(crate) struct Services<'a> {
    pub policy: &'a Policy,
    pub call: &'a api::Call,
    pub cancel: &'a AtomicBool,
}
impl api::Host for Services<'_> {
    fn resolve(&self, v: &api::variables::Variables) -> Result<api::variables::Resolved> {
        if let Some(inputs) = &self.policy.runtime_inputs {
            let mut result = api::variables::Resolved::default();
            for (name, value) in v {
                let text = match value {
                    api::variables::Value::Literal(text) => text.clone(),
                    _ => inputs
                        .values
                        .get(name)
                        .context("declared Secret input missing")?
                        .clone(),
                };
                result.values.insert(name.clone(), text);
            }
            result.redactions = inputs.redactions.clone();
            return Ok(result);
        }
        crate::credentials::secrets::resolve_literals(v)
    }
    fn secret(&self, path: &Path) -> Result<String> {
        api::local::secret(path)
    }
    fn private_directory(&self, path: &Path) -> Result<()> {
        crate::credentials::security::private_state_directory(path)
    }
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let state = self
            .policy
            .runtime_state_dir
            .as_ref()
            .context("runtime state directory missing")?;
        ensure!(
            path.starts_with(state),
            "plugin private output outside runtime directory"
        );
        crate::nodes::agent::durable(path, bytes)
    }
    fn ensure_file(&self, path: &Path, bytes: &[u8], mode: u32) -> Result<bool> {
        ensure!(
            self.call
                .resources
                .contains(&format!("file{}", path.display())),
            "file write not declared by plugin"
        );
        let mut policy = self.policy.clone();
        policy.file_paths.push(path.into());
        use sha2::{Digest, Sha256};
        let operation = Operation::File {
            path: path.to_string_lossy().into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            mode,
        };
        Ok(
            crate::executors::native::reconcile_file(&policy, &operation, bytes, self.cancel)?
                .changed,
        )
    }
}

pub fn validate_plan(plan: &converyn_core::model::domain::Plan) -> Result<()> {
    plan.verify()?;
    for work in &plan.work {
        validate_script(&work.operation)?;
        if let Operation::Plugin { call } = &work.operation {
            registry().validate(call)?;
            if call.location == Location::Controller
                && let Some(target) = &plan.input.target
            {
                ensure!(
                    registry()
                        .get(&call.plugin.id)?
                        .target_binding(call)
                        .as_ref()
                        == Some(&target.spec),
                    "plugin target binding differs from registered target"
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn execute_node(
    policy: &Policy,
    operation: &Operation,
    call: &api::Call,
    cancel: &AtomicBool,
) -> Result<Evidence> {
    registry().validate(call)?;
    ensure!(
        call.location == Location::Node,
        "controller plugin cannot execute on a Node"
    );
    let grant = policy
        .plugins
        .get(&call.plugin.id)
        .context("plugin is not granted locally")?;
    let project = policy
        .runtime_project
        .as_deref()
        .context("plugin requires a project")?;
    ensure!(
        grant.identity == call.plugin
            && grant.projects.contains(project)
            && grant.actions.contains(&call.action),
        "local plugin identity/project/action grant mismatch"
    );
    let host = Services {
        policy,
        call,
        cancel,
    };
    let context = api::Context {
        project,
        owner: policy
            .runtime_owner
            .as_deref()
            .context("plugin owner missing")?,
        state_dir: policy
            .runtime_state_dir
            .as_deref()
            .context("plugin runtime directory missing")?,
        policy: &grant.policy,
        target: None,
        fence: policy.runtime_fence,
        cancel,
        host: &host,
    };
    let result = registry().get(&call.plugin.id)?.apply(call, &context)?;
    let verified = registry().get(&call.plugin.id)?.verify_bound(
        call,
        &result.payload,
        context.owner,
        context.fence,
    )?;
    ensure!(verified, "plugin postcondition has not been verified");
    let mut evidence = Evidence::empty(operation);
    evidence.changed = result.changed;
    evidence.plugin = Some(api::Observation {
        owner: context.owner.into(),
        fence: context.fence,
        plugin: call.plugin.clone(),
        call_sha256: api::digest(call),
        payload: result.payload,
        verified,
    });
    Ok(evidence)
}

/// 控制端本地连接配置，不进入目标登记或 Raft。使用绝对凭据路径。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionGrant {
    pub identity: api::Identity,
    #[serde(default = "api::default_projects")]
    pub projects: BTreeSet<String>,
    pub targets: BTreeSet<String>,
    pub policy: Value,
}

/// 写入共识日志前验证插件语义，复制时只重放确定的调用和证据。
pub(crate) fn validate_command(
    command: &converyn_core::orchestration::machine::Command,
) -> Result<()> {
    use converyn_core::orchestration::machine::Command;
    match command {
        Command::Authorized { command, .. } => validate_command(command),
        Command::CreateRun { plan, .. } | Command::ApplyDeployment { plan, .. } => {
            validate_plan(plan)
        }
        Command::Administer {
            change: converyn_core::orchestration::administration::Change::Target { target },
        } => {
            target.validate()?;
            registry()
                .get(&target.plugin.id)?
                .validate_target(&target.spec)?;
            let descriptor = registry().get(&target.plugin.id)?.descriptor();
            ensure!(
                descriptor.identity == target.plugin && descriptor.location == Location::Controller,
                "unsupported target plugin identity"
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn authorize_node(
    policy: &Policy,
    call: &api::Call,
    project: Option<&str>,
) -> Result<()> {
    registry().validate(call)?;
    ensure!(call.location == Location::Node, "not a Node plugin");
    let grant = policy
        .plugins
        .get(&call.plugin.id)
        .context("plugin is not granted locally")?;
    ensure!(
        grant.identity == call.plugin
            && grant
                .projects
                .contains(project.context("plugin project missing")?)
            && grant.actions.contains(&call.action),
        "local plugin identity/project/action grant mismatch"
    );
    registry()
        .get(&call.plugin.id)?
        .authorize(call, &grant.policy, project.unwrap())?;
    Ok(())
}

fn validate_script(operation: &Operation) -> Result<()> {
    if let Operation::Exec {
        origin: Some(origin),
        script_sha256,
        resources,
        ..
    } = operation
    {
        let plugin = registry().get(&origin.plugin.id)?;
        ensure!(
            plugin.descriptor().identity == origin.plugin
                && plugin.contains_script(&origin.action, script_sha256)
                && plugin.script_resources(&origin.action) == *resources,
            "script differs from compiled plugin package"
        );
    }
    Ok(())
}

/// 只用于操作员本地准备私有输入，不能从 HTTP 或 Raft 命令调用。
pub struct Preparation;
impl api::PreparationHost for Preparation {
    fn create_authority(&self, directory: &Path, name: &str) -> Result<api::PreparedAuthority> {
        let a = crate::credentials::pki::create_authority(directory, name)?;
        Ok(api::PreparedAuthority {
            cert: a.cert,
            key: a.key,
        })
    }
    fn describe_certificate(&self, pem: &[u8]) -> Result<api::CertificateSummary> {
        use sha2::{Digest, Sha256};
        use x509_parser::prelude::FromDer;
        let info = crate::credentials::pki::info(std::str::from_utf8(pem)?)?;
        let mut reader = pem;
        let der = rustls_pemfile::certs(&mut reader)
            .next()
            .context("certificate missing")??;
        let (_, certificate) = x509_parser::certificate::X509Certificate::from_der(der.as_ref())
            .map_err(|_| anyhow::anyhow!("invalid certificate"))?;
        Ok(api::CertificateSummary {
            is_ca: info.is_ca,
            can_sign: info.can_sign,
            expires_ms: info.expires_ms,
            discovery_pin: hex::encode(Sha256::digest(certificate.tbs_certificate.subject_pki.raw)),
        })
    }
    fn random_token(&self) -> Result<String> {
        crate::credentials::pki::random_token()
    }
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        crate::credentials::pki::private(path, bytes)
    }
    fn now_ms(&self) -> u64 {
        crate::executors::host::now_ms()
    }
}
