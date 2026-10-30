# Publish to an existing Kubernetes cluster

[English](README.md) | [简体中文](README.zh-CN.md)

Register [workflow.yaml](workflow.yaml) directly to publish a ConfigMap through the Kubernetes API. Controllers execute this workflow; Kubernetes nodes need no Converyn Agents or host Cluster. Use matching `full` builds and personal contexts with target/workflow registration and execution permissions.

## Prepare the connection

Follow [connection and dedicated credentials](../../docs/en/how-to/external-kubernetes.md) to review/apply [rbac.yaml](rbac.yaml), create a private ServiceAccount kubeconfig, and install it on **every controller**. The controller must reach the API directly. Do not package credentials in the workflow.

Read the real target identity with those dedicated credentials:

```bash
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get namespace kube-system -o jsonpath='{.metadata.uid}{"\n"}'
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get namespace converyn-demo -o jsonpath='{.metadata.uid}{"\n"}'
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get --raw /version
```

Replace all three identity placeholders in [spec.yaml](spec.yaml). The physical cluster UID is the `kube-system` Namespace UID. Edit the ConfigMap data as desired. [connection-policy.json](connection-policy.json) fixes the controller-local kubeconfig path/context and namespace grants; adjust them to your environment.

## Prepare public registrations

```bash
mkdir -p .local/external-kubernetes
converyn -o json validate workflows --file example/external-kubernetes > .local/external-kubernetes/checked.json
jq --slurpfile policy example/external-kubernetes/connection-policy.json '
  .requirements.steps[0].operation.call as $call |
  {target:{action:"target",target:{uid:"k8s-demo",revision:1,project:"default",
    plugin:$call.plugin,connection:"k8s-qa",spec:$call.spec.target}},
   server:{plugin_connections:{"k8s-qa":{identity:$call.plugin,projects:["default"],
    targets:["k8s-demo"],policy:$policy[0]}}}}
' .local/external-kubernetes/checked.json > .local/external-kubernetes/setup.json
jq '.target' .local/external-kubernetes/setup.json > .local/external-kubernetes/target.json
jq '.server' .local/external-kubernetes/setup.json > .local/external-kubernetes/server-fragment.json
```

Review `target.json` and merge `server-fragment.json` into each controller's existing configuration. Preserve member identity, TLS, state and other connections. Install the referenced private kubeconfig separately, restart idle controllers one at a time, and check status. These commands generate only public target/grant data; the workflow itself is already complete.

## Deploy and verify

```bash
converyn --context qa-op update targets --file .local/external-kubernetes/target.json
converyn --context qa-op register workflows --file example/external-kubernetes
converyn --context qa-alice run workflows external-k8s-demo --target qa --dry-run
converyn --context qa-alice run workflows external-k8s-demo --target qa --wait
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa -n converyn-demo get configmap converyn-demo-config -o yaml
```

Inspect `converyn get runs RUN_ID` for controller plugin evidence. The actual ConfigMap should contain the message and `converyn.io/owner`, `converyn.io/fence`, `converyn.io/intent` annotations. For later content changes, edit `spec.yaml` and re-register/run the workflow; do not recreate the unchanged target. Namespace recreation or a different physical cluster requires a new target UID and matching grants.

## Deploy an application

[deployment.yaml](deployment.yaml) uses [deployment-spec.yaml](deployment-spec.yaml) with a Deployment, Service and bounded replicas parameter. Copy the observed identity into that spec, review its pinned BusyBox image, and apply the dedicated [application RBAC](application-rbac.yaml), which also permits rollout watching and service port-forward verification:

```bash
kubectl --kubeconfig /secure/k8s-admin.yaml --context qa apply -f example/external-kubernetes/application-rbac.yaml
converyn validate workflows --file example/external-kubernetes/deployment.yaml
converyn --context qa-op register workflows --file example/external-kubernetes/deployment.yaml
converyn --context qa-alice run workflows external-k8s-web-demo --target qa --set replicas=2 --dry-run
converyn --context qa-alice run workflows external-k8s-web-demo --target qa --set replicas=2 --wait
```

Use `kubectl --kubeconfig /secure/kubernetes/qa.json --context qa rollout status deployment/converyn-demo-web -n converyn-demo` with the prepared kubeconfig to verify workload readiness. Both workflows share target `k8s-demo` and connection `k8s-qa`. For installing Kubernetes itself, see [foundation deployment](../kubernetes/README.md).

## Upgrade the component and restore it

`image` accepts two digest-pinned BusyBox versions; `release_version` controls the HTTP application version. This replaces the actual container image rather than only changing replicas. Publish v1, upgrade BusyBox with v2, then restore v1.

```bash
converyn --context qa-alice run workflows external-k8s-web-demo --target qa --set release_version=v1 --wait
converyn --context qa-alice run workflows external-k8s-web-demo --target qa \
  --set release_version=v2 \
  --set image=docker.io/library/busybox:1.38.0@sha256:fd7dc98638c8e305f4dc34e979f1c0fdfdcaeb0fbf8fcff77ae834b6da3d7e6e --wait
converyn --context qa-alice run workflows external-k8s-web-demo --target qa \
  --set release_version=v1 \
  --set image=docker.io/library/busybox:1.37.0@sha256:bdf57e528e45e4433820e045b29b4597825a1c9e38353532d90a01445013f82e --wait
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa -n converyn-demo rollout status deployment/converyn-demo-web
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa -n converyn-demo get deployment converyn-demo-web -o jsonpath='{.spec.template.spec.containers[0].image}'
```

Use `kubectl --kubeconfig /secure/kubernetes/qa.json --context qa port-forward -n converyn-demo service/converyn-demo-web 18082:80`, then `curl --fail http://127.0.0.1:18082/` in another terminal to verify the selected v1/v2 response. Check resource/readiness evidence in the Run too. See [CI integration](../ci/README.md).

Cluster nodes must reach both pinned digests or have an audited OCI archive retaining those original digests. A cached tag with a different repackaged digest is insufficient. Fix image availability first, then inspect the original Run and workload; handle `needs_attention` through [Run recovery](../../docs/en/how-to/runs.md).
