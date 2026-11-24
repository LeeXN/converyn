use converyn_plugin_api::Plugin;
use converyn_plugin_kubernetes::{Kubernetes, kube_api, kube_resource::Intent};
use serde_json::json;
fn intent() -> Intent {
    Intent {
        owner: "demo".into(),
        fence: 2,
        namespace: "default".into(),
        object: json!({"apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":"demo"},"spec":{"replicas":2}}),
    }
}
fn observed(i: &Intent) -> serde_json::Value {
    let mut v = i.object.clone();
    v["metadata"]["uid"] = "real-uid".into();
    v["metadata"]["resourceVersion"] = "12".into();
    v["metadata"]["generation"] = 3.into();
    v["metadata"]["annotations"] = json!({"converyn.io/owner":i.owner,"converyn.io/fence":i.fence.to_string(),"converyn.io/intent":converyn_plugin_api::digest(&i.object)});
    v["status"] =
        json!({"observedGeneration":3,"replicas":2,"updatedReplicas":2,"availableReplicas":2});
    v
}
#[test]
fn generation_alone_is_not_completion() {
    let i = intent();
    let mut v = observed(&i);
    assert!(i.observe(&v).unwrap().ready);
    v["status"]["updatedReplicas"] = 1.into();
    assert!(!i.observe(&v).unwrap().ready);
    v["status"]["updatedReplicas"] = 2.into();
    v["status"]["observedGeneration"] = 2.into();
    assert!(!i.observe(&v).unwrap().ready);
    v["status"]["observedGeneration"] = 3.into();
    v["status"]["replicas"] = 3.into();
    assert!(!i.observe(&v).unwrap().ready);
}
#[test]
fn stale_owner_and_conflicting_fences_are_rejected() {
    let i = intent();
    let mut v = observed(&i);
    v["metadata"]["annotations"]["converyn.io/fence"] = "3".into();
    assert!(i.observe(&v).is_err());
    v = observed(&i);
    v["metadata"]["annotations"]["converyn.io/owner"] = "other".into();
    assert!(i.observe(&v).is_err());
    v = observed(&i);
    v["metadata"]["annotations"]["converyn.io/intent"] = "other".into();
    assert!(i.observe(&v).is_err());
}
#[test]
fn imported_server_state_and_namespace_escape_are_rejected() {
    let mut i = intent();
    i.object["metadata"]["namespace"] = "other".into();
    assert!(i.validate().is_err());
    i = intent();
    i.object["metadata"]["resourceVersion"] = "12".into();
    assert!(i.validate().is_err());
}

#[test]
fn job_requires_completion_and_uses_existing_workload_guards() {
    let mut i = intent();
    i.object = json!({
        "apiVersion": "batch/v1", "kind": "Job", "metadata": {"name": "demo"},
        "spec": {"template": {"spec": {
            "restartPolicy": "Never",
            "containers": [{"name": "demo", "image": format!("test/demo@sha256:{}", "a".repeat(64))}]
        }}}
    });
    assert_eq!(
        kube_api::path(&i.object, "default").unwrap(),
        "/apis/batch/v1/namespaces/default/jobs/demo"
    );
    let spec = |object: &serde_json::Value| {
        json!({
            "target": {"namespace": "default", "identity": {"cluster_uid": "cluster", "namespace_uid": "namespace", "version": "v1"}},
            "objects": [object],
            "timeout_seconds": 10
        })
    };
    let call = Kubernetes.compile("apply", spec(&i.object)).unwrap();
    let mut active = observed(&i);
    active["status"] = json!({"succeeded": 1});
    assert!(!i.observe(&active).unwrap().ready);
    active["status"]["conditions"] = json!([{"type": "Complete", "status": "True"}]);
    let ready = i.observe(&active).unwrap();
    assert!(ready.ready);
    assert_eq!(ready.observed_generation, 0);
    assert!(Kubernetes.verify(&call, &json!({"identity": call.spec["target"]["identity"], "fence": i.fence, "resources": [ready]})).unwrap());
    active["status"]["conditions"] = json!([{"type": "Failed", "status": "True"}]);
    assert!(!i.observe(&active).unwrap().ready);
    let connection = kube_api::Connection {
        kubeconfig: "/unused".into(),
        context: None,
        projects: ["default".into()].into(),
        namespaces: ["default".into()].into(),
        allow_cluster_resources: false,
        allow_privileged_workloads: false,
    };
    assert!(connection.validate_object(&i.object).is_ok());
    i.object["spec"]["template"]["spec"]["hostNetwork"] = true.into();
    assert!(connection.validate_object(&i.object).is_err());
    i.object["spec"]["template"]["spec"]["containers"][0]["image"] = "test/demo:latest".into();
    assert!(Kubernetes.compile("apply", spec(&i.object)).is_err());
}

#[test]
fn network_policy_verifies_intent_without_status() {
    let mut i = intent();
    i.object = json!({
        "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy", "metadata": {"name": "demo"},
        "spec": {"podSelector": {"matchLabels": {"app": "demo"}}, "policyTypes": ["Ingress"]}
    });
    assert_eq!(
        kube_api::path(&i.object, "default").unwrap(),
        "/apis/networking.k8s.io/v1/namespaces/default/networkpolicies/demo"
    );
    let call = Kubernetes.compile("apply", json!({
        "target": {"namespace": "default", "identity": {"cluster_uid": "cluster", "namespace_uid": "namespace", "version": "v1"}},
        "objects": [i.object],
        "timeout_seconds": 10
    })).unwrap();
    let mut actual = observed(&i);
    actual.as_object_mut().unwrap().remove("status");
    let ready = i.observe(&actual).unwrap();
    assert!(ready.ready);
    assert!(Kubernetes.verify(&call, &json!({"identity": call.spec["target"]["identity"], "fence": i.fence, "resources": [ready]})).unwrap());
    actual["spec"]["podSelector"]["matchLabels"]["app"] = "other".into();
    assert!(i.observe(&actual).is_err());
}
#[tokio::test]
#[ignore = "requires explicit local Kubernetes fixture"]
async fn actual_api_conflicts_and_ownership() {
    assert_eq!(std::env::var("CONVERYN_KUBE_TEST").unwrap(), "owned-lab");
    let connection = converyn_plugin_kubernetes::kube_api::Connection {
        kubeconfig: std::env::var("CONVERYN_KUBECONFIG").unwrap().into(),
        context: None,
        projects: ["lab".into()].into(),
        namespaces: ["default".into()].into(),
        allow_cluster_resources: false,
        allow_privileged_workloads: false,
    };
    let client = connection.client().unwrap();
    let identity = client.identity("default").await.unwrap();
    assert!(!identity.cluster_uid.is_empty());
    let name = format!("converyn-api-{}", std::process::id());
    let mut i = Intent {
        owner: name.clone(),
        fence: 1,
        namespace: "default".into(),
        object: json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":name},"data":{"version":"one"}}),
    };
    let first = i.apply(&client).await.unwrap();
    assert!(i.observe(&first).unwrap().ready);
    let same = i.apply(&client).await.unwrap();
    assert_eq!(same["metadata"]["uid"], first["metadata"]["uid"]);
    let old = i.clone();
    i.fence = 2;
    i.object["data"]["version"] = "two".into();
    let second = i.apply(&client).await.unwrap();
    assert_eq!(second["data"]["version"], "two");
    assert!(old.apply(&client).await.is_err());
    i.owner = "foreign".into();
    assert!(i.apply(&client).await.is_err());
    let path = converyn_plugin_kubernetes::kube_api::path(&old.object, "default").unwrap();
    let delete = json!({"apiVersion":"v1","kind":"DeleteOptions","preconditions":{"uid":second["metadata"]["uid"],"resourceVersion":second["metadata"]["resourceVersion"]}});
    let (status, _) = client
        .request(reqwest::Method::DELETE, &path, Some(&delete), false)
        .await
        .unwrap();
    assert!((200..300).contains(&status));
    if let Ok(output) = std::env::var("CONVERYN_KUBE_RESULT") {
        std::fs::write(output, serde_json::to_vec_pretty(&json!({"cluster":identity,"create":true,"same_intent":true,"upgrade":true,"stale_fence_rejected":true,"foreign_owner_rejected":true,"cleanup_uid_cas":true})).unwrap()).unwrap();
    }
}
