//! The controller supplies a committed monotonic fence. Kubernetes CAS prevents
//! an older controller request from replacing a newer committed delivery.
use crate::kube_api::{Client, path, resource};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
const OWNER: &str = "converyn.io/owner";
const FENCE: &str = "converyn.io/fence";
const DIGEST: &str = "converyn.io/intent";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Intent {
    pub owner: String,
    pub fence: u64,
    pub namespace: String,
    pub object: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub uid: String,
    pub resource_version: String,
    pub generation: u64,
    pub observed_generation: u64,
    pub ready: bool,
}
impl Intent {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            converyn_plugin_api::valid_id(&self.owner) && self.fence > 0,
            "invalid owner or fence"
        );
        path(&self.object, &self.namespace)?;
        let meta = &self.object["metadata"];
        ensure!(
            meta.get("namespace").is_none_or(|v| v == &self.namespace),
            "namespace mismatch"
        );
        for field in [
            "uid",
            "resourceVersion",
            "generation",
            "managedFields",
            "deletionTimestamp",
            "ownerReferences",
            "generateName",
        ] {
            ensure!(
                meta.get(field).is_none(),
                "server-managed metadata is forbidden: {field}"
            );
        }
        ensure!(
            self.object.get("status").is_none(),
            "status is not desired configuration"
        );
        for key in [OWNER, FENCE, DIGEST] {
            ensure!(
                meta["annotations"].get(key).is_none(),
                "reserved annotation"
            );
        }
        ensure!(
            serde_json::to_vec(&self.object)?.len() <= 1024 * 1024,
            "object exceeds 1 MiB"
        );
        Ok(())
    }
    fn desired(&self) -> Result<Value> {
        self.validate()?;
        let mut object = self.object.clone();
        if object["metadata"].get("annotations").is_none() {
            object["metadata"]["annotations"] = json!({});
        }
        let annotations = object["metadata"]["annotations"]
            .as_object_mut()
            .context("annotations must be a map")?;
        annotations.insert(OWNER.into(), self.owner.clone().into());
        annotations.insert(FENCE.into(), self.fence.to_string().into());
        annotations.insert(
            DIGEST.into(),
            converyn_plugin_api::digest(&self.object).into(),
        );
        let (_, cluster) = resource(
            self.object["apiVersion"].as_str().unwrap(),
            self.object["kind"].as_str().unwrap(),
        )
        .unwrap();
        if !cluster {
            object["metadata"]["namespace"] = self.namespace.clone().into();
        }
        Ok(object)
    }
    fn check_owner(&self, existing: &Value) -> Result<()> {
        let annotations = &existing["metadata"]["annotations"];
        ensure!(
            annotations[OWNER] == self.owner,
            "resource belongs to another owner"
        );
        let fence: u64 = annotations[FENCE]
            .as_str()
            .context("resource fence missing")?
            .parse()?;
        ensure!(fence <= self.fence, "stale delivery fence");
        if fence == self.fence {
            ensure!(
                annotations[DIGEST] == converyn_plugin_api::digest(&self.object),
                "same fence with different intent"
            );
        }
        ensure!(
            existing["metadata"].get("deletionTimestamp").is_none(),
            "resource is being deleted"
        );
        Ok(())
    }
    /// No force takeover and no retry on a CAS conflict. The caller must obtain
    /// quorum again before attempting another mutation.
    pub async fn apply(&self, client: &Client) -> Result<Value> {
        let mut desired = self.desired()?;
        let path = path(&self.object, &self.namespace)?;
        let (status, existing) = client
            .request(reqwest::Method::GET, &path, None, false)
            .await?;
        let (status, result) = match status {
            404 => {
                let collection = path
                    .rsplit_once('/')
                    .context("resource collection missing")?
                    .0;
                client
                    .request(
                        reqwest::Method::POST,
                        &format!("{collection}?fieldManager=converyn"),
                        Some(&desired),
                        false,
                    )
                    .await?
            }
            200 => {
                self.check_owner(&existing)?;
                // POST is create-only, avoiding an SSA create race against a
                // foreign object. Retire only our POST field ownership before
                // SSA; Update and Apply managers otherwise conflict on upgrade.
                let existing = if let Some(fields) =
                    existing["metadata"]["managedFields"].as_array()
                {
                    let mut patch = vec![
                        json!({"op":"test","path":"/metadata/resourceVersion","value":existing["metadata"]["resourceVersion"]}),
                    ];
                    for (index, field) in fields.iter().enumerate().rev() {
                        if field["manager"] == "converyn" && field["operation"] == "Update" {
                            patch.push(json!({"op":"replace","path":format!("/metadata/managedFields/{index}/operation"),"value":"Apply"}));
                        }
                    }
                    if patch.len() > 1 {
                        let (status, value) = client
                            .request(
                                reqwest::Method::PATCH,
                                &path,
                                Some(&Value::Array(patch)),
                                false,
                            )
                            .await?;
                        ensure!(
                            status == 200,
                            "field ownership transition conflict (HTTP {status})"
                        );
                        value
                    } else {
                        existing
                    }
                } else {
                    existing
                };
                desired["metadata"]["resourceVersion"] =
                    existing["metadata"]["resourceVersion"].clone();
                client
                    .request(
                        reqwest::Method::PATCH,
                        &format!("{path}?fieldManager=converyn&force=false"),
                        Some(&desired),
                        true,
                    )
                    .await?
            }
            _ => anyhow::bail!("resource lookup failed (HTTP {status})"),
        };
        ensure!(
            (200..300).contains(&status),
            "resource apply failed (HTTP {status}); ownership or version conflict requires reconciliation"
        );
        self.check_owner(&result)?;
        Ok(result)
    }
    pub fn observe(&self, object: &Value) -> Result<Evidence> {
        self.check_owner(object)?;
        ensure!(
            object["metadata"]["annotations"][FENCE]
                .as_str()
                .and_then(|v| v.parse::<u64>().ok())
                == Some(self.fence),
            "desired fence has not been applied"
        );
        ensure!(
            crate::kube_api::subset(&self.object, object),
            "live configuration differs from desired intent"
        );
        let generation = object["metadata"]["generation"].as_u64().unwrap_or(0);
        let observed = object["status"]["observedGeneration"].as_u64().unwrap_or(0);
        let count = |key: &str| object["status"][key].as_u64().unwrap_or(0);
        let replicas = object["spec"]["replicas"].as_u64().unwrap_or(1);
        let ready = match object["kind"].as_str().unwrap_or("") {
            "Deployment" => {
                observed >= generation
                    && count("updatedReplicas") == replicas
                    && count("availableReplicas") == replicas
                    && count("replicas") == replicas
                    && count("unavailableReplicas") == 0
            }
            "StatefulSet" => {
                observed >= generation
                    && count("readyReplicas") == replicas
                    && count("updatedReplicas") == replicas
                    && object["status"]["currentRevision"].is_string()
                    && object["status"]["currentRevision"] == object["status"]["updateRevision"]
            }
            "DaemonSet" => {
                observed >= generation
                    && count("desiredNumberScheduled") > 0
                    && count("updatedNumberScheduled") == count("desiredNumberScheduled")
                    && count("numberAvailable") == count("desiredNumberScheduled")
                    && count("numberUnavailable") == 0
                    && count("numberMisscheduled") == 0
            }
            "CustomResourceDefinition" => {
                object["status"]["conditions"].as_array().is_some_and(|c| {
                    c.iter()
                        .any(|c| c["type"] == "Established" && c["status"] == "True")
                })
            }
            _ => true,
        };
        Ok(Evidence {
            uid: object["metadata"]["uid"]
                .as_str()
                .context("UID missing")?
                .into(),
            resource_version: object["metadata"]["resourceVersion"]
                .as_str()
                .context("resourceVersion missing")?
                .into(),
            generation,
            observed_generation: observed,
            ready,
        })
    }
}
