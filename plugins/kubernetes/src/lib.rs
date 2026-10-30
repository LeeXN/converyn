//! Kubernetes 工作负载 CD 插件。只处理外部 API 与资源证据；审批和历史属于核心。
pub mod kube_api;
pub mod kube_resource;
use anyhow::{Context as _, Result, ensure};
use converyn_plugin_api::{self as api, Plugin};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
/// 目标身份由管理者登记，凭据仅通过 connection 引用控制端本地配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub namespace: String,
    pub identity: kube_api::Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub target: Target,
    pub objects: Vec<Value>,
    pub timeout_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub identity: kube_api::Identity,
    pub fence: u64,
    pub resources: Vec<kube_resource::Evidence>,
}
pub struct Kubernetes;
impl Kubernetes {
    async fn run(&self, call: &api::Call, c: &api::Context<'_>, apply: bool) -> Result<Value> {
        let spec: Spec = serde_json::from_value(call.spec.clone())?;
        let target: Target =
            serde_json::from_value(c.target.context("external target missing")?.clone())?;
        ensure!(
            target == spec.target,
            "registered target differs from approved target"
        );
        let connection: kube_api::Connection = serde_json::from_value(c.policy.clone())?;
        ensure!(
            connection.allowed(c.project, &target.namespace),
            "Kubernetes connection scope denied"
        );
        for object in &spec.objects {
            connection.validate_object(object)?;
        }
        let client = connection.client()?;
        let identity = client.identity(&target.namespace).await?;
        ensure!(
            identity.cluster_uid == target.identity.cluster_uid
                && identity.namespace_uid == target.identity.namespace_uid,
            "physical Kubernetes target identity changed"
        );
        let mut intents = Vec::new();
        for object in &spec.objects {
            intents.push(kube_resource::Intent {
                owner: c.owner.into(),
                fence: c.fence,
                namespace: target.namespace.clone(),
                object: object.clone(),
            });
        }
        if apply {
            for intent in &intents {
                ensure!(
                    !c.cancel.load(Ordering::SeqCst),
                    "Kubernetes operation canceled"
                );
                intent.apply(&client).await?;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(u64::from(spec.timeout_seconds));
        loop {
            ensure!(
                !c.cancel.load(Ordering::SeqCst),
                "Kubernetes operation canceled"
            );
            let mut evidence = Vec::new();
            let mut complete = true;
            for intent in &intents {
                let path = kube_api::path(&intent.object, &target.namespace)?;
                let observed = client.get(&path).await.and_then(|v| {
                    let mut expected = intent.clone();
                    if !apply {
                        let old = v["metadata"]["annotations"]["converyn.io/fence"]
                            .as_str()
                            .and_then(|s| s.parse::<u64>().ok())
                            .context("resource fence missing")?;
                        ensure!(
                            old > 0 && old <= c.fence,
                            "newer external fence blocks this intent"
                        );
                        expected.fence = old;
                    }
                    expected.observe(&v)
                });
                match observed {
                    Ok(e) => {
                        complete &= e.ready;
                        evidence.push(e)
                    }
                    Err(e) if !apply => return Err(e),
                    Err(_) => complete = false,
                }
            }
            if complete {
                return Ok(serde_json::to_value(Evidence {
                    identity,
                    fence: c.fence,
                    resources: evidence,
                })?);
            }
            ensure!(
                apply && Instant::now() < deadline,
                "Kubernetes desired state has not become ready"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
impl Plugin for Kubernetes {
    fn validate_target(&self, target: &Value) -> Result<()> {
        let target: Target = serde_json::from_value(target.clone())?;
        ensure!(
            kube_api::dns_name(&target.namespace)
                && api::valid_id(&target.identity.cluster_uid)
                && api::valid_id(&target.identity.namespace_uid)
                && target.identity.version.len() <= 128,
            "invalid public Kubernetes target identity"
        );
        Ok(())
    }
    fn descriptor(&self) -> api::Descriptor {
        api::Descriptor {
            identity: api::Identity {
                id: "converyn.kubernetes".into(),
                version: "1.0.0".into(),
                contract: 1,
                sha256: CONTENT_SHA256.into(),
            },
            location: api::Location::Controller,
            actions: vec!["apply".into()],
            description: "Kubernetes manifest delivery and observed readiness".into(),
        }
    }
    fn compile(&self, action: &str, spec: Value) -> Result<api::Call> {
        ensure!(action == "apply", "unknown Kubernetes action");
        let s: Spec = serde_json::from_value(spec.clone())?;
        ensure!(
            kube_api::dns_name(&s.target.namespace)
                && !s.target.identity.cluster_uid.is_empty()
                && !s.target.identity.namespace_uid.is_empty()
                && !s.objects.is_empty()
                && s.objects.len() <= 128
                && (10..=7200).contains(&s.timeout_seconds),
            "invalid Kubernetes specification"
        );
        let mut resources = BTreeSet::new();
        for object in &s.objects {
            let intent = kube_resource::Intent {
                owner: "validation".into(),
                fence: 1,
                namespace: s.target.namespace.clone(),
                object: object.clone(),
            };
            intent.validate()?;
            let path = kube_api::path(object, &s.target.namespace)?;
            ensure!(
                resources.insert(format!(
                    "external/{}/{}{}",
                    s.target.identity.cluster_uid, s.target.identity.namespace_uid, path
                )),
                "duplicate Kubernetes resource"
            );
            if let Some(pod) = object.pointer("/spec/template/spec") {
                for field in ["containers", "initContainers"] {
                    if let Some(containers) = pod[field].as_array() {
                        for c in containers {
                            let image = c["image"].as_str().context("image missing")?;
                            ensure!(
                                image
                                    .rsplit_once("@sha256:")
                                    .is_some_and(|(_, sha)| api::valid_sha(sha)),
                                "container image must be pinned by digest"
                            );
                        }
                    }
                }
            }
        }
        Ok(api::Call {
            plugin: self.descriptor().identity,
            action: action.into(),
            location: api::Location::Controller,
            spec,
            resources: resources.into_iter().collect(),
            reconcilable: true,
            timeout_seconds: s.timeout_seconds,
        })
    }
    fn authorize(&self, call: &api::Call, policy: &Value, project: &str) -> Result<()> {
        self.validate(call)?;
        let s: Spec = serde_json::from_value(call.spec.clone())?;
        let c: kube_api::Connection = serde_json::from_value(policy.clone())?;
        ensure!(
            c.kubeconfig.is_absolute(),
            "local kubeconfig path must be absolute"
        );
        ensure!(
            c.allowed(project, &s.target.namespace),
            "Kubernetes connection scope denied"
        );
        for object in &s.objects {
            c.validate_object(object)?;
        }
        Ok(())
    }
    fn apply(&self, call: &api::Call, c: &api::Context<'_>) -> Result<api::Applied> {
        self.validate(call)?;
        let payload = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(self.run(call, c, true))?;
        Ok(api::Applied {
            changed: true,
            payload,
        })
    }
    fn observe(&self, call: &api::Call, c: &api::Context<'_>) -> Result<Value> {
        self.validate(call)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(self.run(call, c, false))
    }
    fn verify_bound(
        &self,
        call: &api::Call,
        payload: &Value,
        _owner: &str,
        fence: u64,
    ) -> Result<bool> {
        let e: Evidence = serde_json::from_value(payload.clone())?;
        Ok(e.fence <= fence && self.verify(call, payload)?)
    }
    fn bind_target(&self, call: &api::Call, target: &Value) -> Result<api::Call> {
        self.validate(call)?;
        let mut s: Spec = serde_json::from_value(call.spec.clone())?;
        let next: Target = serde_json::from_value(target.clone())?;
        for object in &mut s.objects {
            let (_, cluster) = kube_api::resource(
                object["apiVersion"].as_str().unwrap_or(""),
                object["kind"].as_str().unwrap_or(""),
            )
            .context("unsupported resource")?;
            if !cluster && object["metadata"].get("namespace").is_some() {
                ensure!(
                    object["metadata"]["namespace"] == s.target.namespace,
                    "manifest namespace differs from source target"
                );
                object["metadata"]["namespace"] = next.namespace.clone().into();
            }
        }
        s.target = next;
        self.compile(&call.action, serde_json::to_value(s)?)
    }
    fn target_binding(&self, call: &api::Call) -> Option<Value> {
        Some(call.spec["target"].clone())
    }
    fn verify(&self, call: &api::Call, payload: &Value) -> Result<bool> {
        let s: Spec = serde_json::from_value(call.spec.clone())?;
        let e: Evidence = serde_json::from_value(payload.clone())?;
        Ok(e.identity.cluster_uid == s.target.identity.cluster_uid
            && e.identity.namespace_uid == s.target.identity.namespace_uid
            && e.fence > 0
            && e.resources.len() == s.objects.len()
            && e.resources.iter().all(|r| {
                r.ready
                    && !r.uid.is_empty()
                    && !r.resource_version.is_empty()
                    && r.observed_generation >= r.generation
            }))
    }
}
