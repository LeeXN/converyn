//! kubeadm 是内嵌脚本包，不是原生执行器。展开后的脚本与参数一同进入审批摘要。
use anyhow::{Context as _, Result, ensure};
use converyn_plugin_api::{self as api, Plugin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
pub mod kit;
pub struct Kubeadm;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub version: String,
    pub environment: api::variables::Variables,
    pub execution: String,
    pub timeout_seconds: u32,
}
const ACTIONS: &[&str] = &[
    "init",
    "join",
    "upgrade_control_plane",
    "upgrade_secondary_control_plane",
    "upgrade_node",
    "check",
    "drain",
    "uncordon",
];
fn source(action: &str) -> Result<String> {
    let body = match action {
        "init" => include_str!("../resources/init.sh"),
        "join" => include_str!("../resources/join.sh"),
        "upgrade_control_plane" => include_str!("../resources/upgrade_control_plane.sh"),
        "upgrade_secondary_control_plane" => {
            include_str!("../resources/upgrade_secondary_control_plane.sh")
        }
        "upgrade_node" => include_str!("../resources/upgrade_node.sh"),
        "check" => include_str!("../resources/check.sh"),
        "drain" => include_str!("../resources/drain.sh"),
        "uncordon" => include_str!("../resources/uncordon.sh"),
        _ => anyhow::bail!("unknown kubeadm action"),
    };
    Ok(format!(
        "{}\n{body}",
        include_str!("../resources/common.sh")
    ))
}
fn sha(s: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(s.as_bytes()))
}
impl Plugin for Kubeadm {
    fn descriptor(&self) -> api::Descriptor {
        api::Descriptor {
            identity: api::Identity {
                id: "converyn.kubeadm".into(),
                version: "1.0.0".into(),
                contract: 1,
                sha256: CONTENT_SHA256.into(),
            },
            location: api::Location::Node,
            actions: ACTIONS.iter().map(|s| s.to_string()).collect(),
            description: "Embedded kubeadm Playbook scripts; no native lifecycle executor".into(),
        }
    }
    fn compile(&self, _action: &str, _spec: Value) -> Result<api::Call> {
        anyhow::bail!("kubeadm must expand to a Playbook before approval")
    }
    fn apply(&self, _call: &api::Call, _c: &api::Context<'_>) -> Result<api::Applied> {
        anyhow::bail!("script packages are not native executors")
    }
    fn verify(&self, _call: &api::Call, _p: &Value) -> Result<bool> {
        Ok(false)
    }
    fn script_resources(&self, _: &str) -> Vec<String> {
        vec!["kubernetes-host".into()]
    }
    fn contains_script(&self, action: &str, digest: &str) -> bool {
        source(action).is_ok_and(|s| sha(&s) == digest)
    }
    fn prepare(
        &self,
        action: &str,
        input: Value,
        directory: &std::path::Path,
        host: &dyn api::PreparationHost,
    ) -> Result<()> {
        let version = input["version"].as_str().context("version missing")?;
        match action {
            "install" => {
                ensure!(
                    version == "v1.36.4",
                    "installation profile verified only for v1.36.4"
                );
                kit::create(
                    host,
                    directory,
                    version,
                    input["endpoint"].as_str().context("endpoint missing")?,
                    input["advertise_address"]
                        .as_str()
                        .context("advertise address missing")?
                        .parse()?,
                    input["pod_cidr"].as_str().context("Pod CIDR missing")?,
                    input["service_cidr"]
                        .as_str()
                        .context("Service CIDR missing")?,
                )
            }
            "upgrade" => {
                ensure!(
                    version == "v1.37.0",
                    "upgrade profile verified only for v1.37.0"
                );
                kit::upgrade(
                    host,
                    directory,
                    version,
                    std::path::Path::new(
                        input["ca_file"]
                            .as_str()
                            .context("existing CA file missing")?,
                    ),
                )
            }
            _ => anyhow::bail!("unknown preparation action"),
        }
    }
    fn expand(&self, action: &str, input: Value) -> Result<api::Expansion> {
        let mut input: Input = serde_json::from_value(input)?;
        ensure!(
            matches!(input.version.as_str(), "v1.36.4" | "v1.37.0"),
            "unsupported kubeadm profile; only tested versions are offered"
        );
        ensure!(
            !(action == "init" || action == "join") || input.version == "v1.36.4",
            "initial installation profile is verified only for v1.36.4"
        );
        ensure!(
            input.execution == "host" && (10..=7200).contains(&input.timeout_seconds),
            "invalid script profile or timeout"
        );
        api::variables::validate(&input.environment)?;
        for key in ["CA_FILE", "KUBECONFIG", "NODE_NAME", "CA_SHA256"] {
            ensure!(
                input.environment.contains_key(key),
                "required environment binding {key} missing"
            );
        }
        if matches!(
            action,
            "init"
                | "join"
                | "upgrade_control_plane"
                | "upgrade_secondary_control_plane"
                | "upgrade_node"
        ) {
            for key in ["BINARY_SHA256", "CONFIG_PATH", "CONFIG_SHA256"] {
                ensure!(
                    input.environment.contains_key(key),
                    "required transaction binding {key} missing"
                );
            }
        }
        if action == "upgrade_secondary_control_plane" {
            ensure!(
                input.environment.contains_key("REFERENCE_KUBECONFIG"),
                "secondary control plane requires an upgraded reference endpoint"
            );
        }
        input
            .environment
            .insert("VERSION".into(), input.version.into());
        if action == "check" {
            input.environment.insert(
                "CHECK_TIMEOUT_SECONDS".into(),
                (input.timeout_seconds.saturating_sub(5).min(300))
                    .to_string()
                    .into(),
            );
        }
        let script = source(action)?;
        let digest = sha(&script);
        let identity = self.descriptor().identity;
        let step = json!({"id":action,"phase":"rollout","scope":"per_node","roles":[],"operation":{"kind":"exec","resources":["kubernetes-host"],"origin":{"plugin":identity,"action":action},"execution":input.execution,"environment":input.environment,"script_sha256":digest,"timeout_seconds":input.timeout_seconds}});
        Ok(api::Expansion {
            plugin: identity,
            action: action.into(),
            steps: vec![step],
            definitions: BTreeMap::from([(digest, script)]),
        })
    }
}
