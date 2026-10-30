//! Kubernetes target transport is separate from host Agents. Credentials stay
//! in controller-local kubeconfig references and are never copied into Plans.
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub kubeconfig: PathBuf,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default = "converyn_plugin_api::default_projects")]
    pub projects: BTreeSet<String>,
    pub namespaces: BTreeSet<String>,
    #[serde(default)]
    pub allow_cluster_resources: bool,
    #[serde(default)]
    pub allow_privileged_workloads: bool,
}
#[derive(Clone)]
pub struct Client {
    pub ca_sha256: String,
    pub http: reqwest::Client,
    pub endpoint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    pub cluster_uid: String,
    pub namespace_uid: String,
    pub version: String,
}
fn data(value: &Value, inline: &str, file: &str, base: &Path, private: bool) -> Result<Vec<u8>> {
    if let Some(encoded) = value[inline].as_str() {
        return Ok(base64::engine::general_purpose::STANDARD.decode(encoded)?);
    }
    let path = Path::new(
        value[file]
            .as_str()
            .context("kubeconfig certificate reference missing")?,
    );
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    if private {
        Ok(converyn_plugin_api::local::secret(&path)?.into_bytes())
    } else {
        Ok(std::fs::read(path)?)
    }
}
impl Connection {
    pub fn allowed(&self, project: &str, namespace: &str) -> bool {
        self.projects.contains(project) && self.namespaces.contains(namespace)
    }
    pub fn validate_object(&self, object: &Value) -> Result<()> {
        let (_, cluster) = resource(
            object["apiVersion"].as_str().unwrap_or(""),
            object["kind"].as_str().unwrap_or(""),
        )
        .context("unsupported Kubernetes resource")?;
        ensure!(
            !cluster || self.allow_cluster_resources,
            "cluster-scoped resource denied"
        );
        if self.allow_privileged_workloads {
            return Ok(());
        }
        if let Some(pod) = object.pointer("/spec/template/spec") {
            ensure!(
                !["hostNetwork", "hostPID", "hostIPC"]
                    .iter()
                    .any(|field| pod[*field] == true),
                "host namespaces require an explicit privileged connection"
            );
            ensure!(
                !pod["volumes"].as_array().is_some_and(|volumes| volumes
                    .iter()
                    .any(|volume| volume.get("hostPath").is_some())),
                "hostPath requires an explicit privileged connection"
            );
            for field in ["containers", "initContainers", "ephemeralContainers"] {
                if let Some(containers) = pod[field].as_array() {
                    ensure!(
                        !containers.iter().any(
                            |container| container["securityContext"]["privileged"] == true
                                || container["securityContext"]["capabilities"]["add"]
                                    .as_array()
                                    .is_some_and(|caps| !caps.is_empty())
                        ),
                        "privileged containers or additional capabilities require an explicit privileged connection"
                    );
                }
            }
        }
        Ok(())
    }
    pub fn client(&self) -> Result<Client> {
        let text = converyn_plugin_api::local::secret(&self.kubeconfig)?;
        ensure!(text.len() <= 1024 * 1024, "kubeconfig exceeds 1 MiB");
        let root: Value = serde_yaml::from_str(&text)?;
        let name = self
            .context
            .as_deref()
            .or(root["current-context"].as_str())
            .context("kubeconfig context missing")?;
        let context = &root["contexts"]
            .as_array()
            .context("kubeconfig contexts missing")?
            .iter()
            .find(|v| v["name"] == name)
            .context("selected kubeconfig context missing")?["context"];
        let cluster = &root["clusters"]
            .as_array()
            .context("kubeconfig clusters missing")?
            .iter()
            .find(|v| v["name"] == context["cluster"])
            .context("kubeconfig cluster missing")?["cluster"];
        let user = &root["users"]
            .as_array()
            .context("kubeconfig users missing")?
            .iter()
            .find(|v| v["name"] == context["user"])
            .context("kubeconfig user missing")?["user"];
        ensure!(
            cluster["insecure-skip-tls-verify"] != true
                && user.get("exec").is_none()
                && user.get("auth-provider").is_none(),
            "insecure TLS and executable credential plugins are not accepted"
        );
        let endpoint = cluster["server"]
            .as_str()
            .context("Kubernetes endpoint missing")?;
        let url = reqwest::Url::parse(endpoint)?;
        ensure!(
            url.scheme() == "https"
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Kubernetes endpoint must be HTTPS without embedded credentials"
        );
        ensure!(
            cluster.get("proxy-url").is_none()
                && cluster["tls-server-name"]
                    .as_str()
                    .is_none_or(|name| Some(name) == url.host_str()),
            "unsupported proxy or alternate TLS server name"
        );
        let base = self
            .kubeconfig
            .parent()
            .context("kubeconfig parent missing")?;
        let ca = data(
            cluster,
            "certificate-authority-data",
            "certificate-authority",
            base,
            false,
        )?;
        let mut client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .tls_built_in_root_certs(false)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(20));
        for cert in reqwest::Certificate::from_pem_bundle(&ca)? {
            client = client.add_root_certificate(cert);
        }
        if user.get("client-certificate-data").is_some() || user.get("client-certificate").is_some()
        {
            let mut cert = data(
                user,
                "client-certificate-data",
                "client-certificate",
                base,
                false,
            )?;
            cert.push(b'\n');
            cert.extend(data(user, "client-key-data", "client-key", base, true)?);
            client = client.identity(reqwest::Identity::from_pem(&cert)?);
        }
        let token = if let Some(token) = user["token"].as_str() {
            Some(token.to_string())
        } else {
            user["tokenFile"]
                .as_str()
                .map(|path| converyn_plugin_api::local::secret(&base.join(path)))
                .transpose()?
        };
        if let Some(token) = token {
            let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
            value.set_sensitive(true);
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(reqwest::header::AUTHORIZATION, value);
            client = client.default_headers(headers);
        }
        use sha2::{Digest, Sha256};
        Ok(Client {
            ca_sha256: hex::encode(Sha256::digest(&ca)),
            http: client.build()?,
            endpoint: endpoint.trim_end_matches('/').into(),
        })
    }
}
impl Client {
    pub async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
        apply: bool,
    ) -> Result<(u16, Value)> {
        ensure!(
            path.starts_with('/') && !path.starts_with("//") && !path.contains(['#', '\n', '\r']),
            "invalid Kubernetes API path"
        );
        let mut request = self
            .http
            .request(method.clone(), format!("{}{path}", self.endpoint));
        if let Some(body) = body {
            request = if apply {
                request
                    .header("Content-Type", "application/apply-patch+yaml")
                    .body(serde_json::to_vec(body)?)
            } else if method == reqwest::Method::PATCH && body.is_array() {
                request
                    .header("Content-Type", "application/json-patch+json")
                    .body(serde_json::to_vec(body)?)
            } else {
                request.json(body)
            };
        }
        let mut response = request.send().await.map_err(|e| e.without_url())?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.without_url())? {
            ensure!(
                bytes.len() + chunk.len() <= 8 * 1024 * 1024,
                "Kubernetes response exceeds 8 MiB"
            );
            bytes.extend_from_slice(&chunk);
        }
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok((status, value))
    }
    pub async fn get(&self, path: &str) -> Result<Value> {
        let (status, value) = self
            .request(reqwest::Method::GET, path, None, false)
            .await?;
        ensure!(status == 200, "Kubernetes read failed (HTTP {status})");
        Ok(value)
    }
    pub async fn identity(&self, namespace: &str) -> Result<Identity> {
        ensure!(dns_name(namespace), "invalid namespace");
        let cluster = self.get("/api/v1/namespaces/kube-system").await?;
        let namespace = self.get(&format!("/api/v1/namespaces/{namespace}")).await?;
        let version = self.get("/version").await?;
        Ok(Identity {
            cluster_uid: cluster["metadata"]["uid"]
                .as_str()
                .context("cluster identity missing")?
                .into(),
            namespace_uid: namespace["metadata"]["uid"]
                .as_str()
                .context("namespace identity missing")?
                .into(),
            version: version["gitVersion"]
                .as_str()
                .context("server version missing")?
                .into(),
        })
    }
}
pub fn dns_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
pub fn resource(api: &str, kind: &str) -> Option<(&'static str, bool)> {
    match (api, kind) {
        ("apps/v1", "Deployment") => Some(("deployments", false)),
        ("apps/v1", "StatefulSet") => Some(("statefulsets", false)),
        ("apps/v1", "DaemonSet") => Some(("daemonsets", false)),
        ("v1", "Service") => Some(("services", false)),
        ("policy/v1", "PodDisruptionBudget") => Some(("poddisruptionbudgets", false)),
        ("v1", "ConfigMap") => Some(("configmaps", false)),
        ("v1", "ServiceAccount") => Some(("serviceaccounts", false)),
        ("networking.k8s.io/v1", "Ingress") => Some(("ingresses", false)),
        ("rbac.authorization.k8s.io/v1", "Role") => Some(("roles", false)),
        ("rbac.authorization.k8s.io/v1", "RoleBinding") => Some(("rolebindings", false)),
        ("rbac.authorization.k8s.io/v1", "ClusterRole") => Some(("clusterroles", true)),
        ("rbac.authorization.k8s.io/v1", "ClusterRoleBinding") => {
            Some(("clusterrolebindings", true))
        }
        ("apiextensions.k8s.io/v1", "CustomResourceDefinition") => {
            Some(("customresourcedefinitions", true))
        }
        _ => None,
    }
}
pub fn path(object: &Value, namespace: &str) -> Result<String> {
    let api = object["apiVersion"]
        .as_str()
        .context("apiVersion missing")?;
    let kind = object["kind"].as_str().context("kind missing")?;
    let name = object["metadata"]["name"]
        .as_str()
        .context("name missing")?;
    ensure!(
        dns_name(name) && dns_name(namespace),
        "invalid Kubernetes object name"
    );
    let (plural, cluster) = resource(api, kind).context("unsupported Kubernetes resource")?;
    let prefix = if api == "v1" {
        "/api/v1".into()
    } else {
        format!("/apis/{api}")
    };
    Ok(if cluster {
        format!("{prefix}/{plural}/{name}")
    } else {
        format!("{prefix}/namespaces/{namespace}/{plural}/{name}")
    })
}
pub fn subset(expected: &Value, actual: &Value) -> bool {
    match expected {
        Value::Object(map) => map
            .iter()
            .all(|(k, v)| actual.get(k).is_some_and(|a| subset(v, a))),
        Value::Array(values) => actual.as_array().is_some_and(|a| {
            a.len() == values.len() && values.iter().zip(a).all(|(e, a)| subset(e, a))
        }),
        _ => expected == actual,
    }
}
