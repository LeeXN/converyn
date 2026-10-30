use converyn_plugin_kubernetes::kube_resource::Intent;
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
