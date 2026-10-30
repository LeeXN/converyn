# Connect an existing external Kubernetes cluster

[English](external-kubernetes.md) | [简体中文](../../zh-CN/how-to/external-kubernetes.md)

Publish a ConfigMap to an existing cluster, then extend the example to a Deployment. External API workflows need neither Agents on Kubernetes nodes nor a host Cluster registration. Converyn controllers access the API Server directly; operator-side `kubectl` prepares and independently verifies the target.

For Kubernetes installation on blank Linux hosts, follow the [infrastructure example](../../../example/kubernetes/README.md) using kubeadm instead.

For a complete native YAML workflow, use the [deployable example](../../../example/external-kubernetes/README.md). The credential preparation below applies to that path too.

## 1. Prepare connection, identity, and permissions

Prerequisites: an installed control plane with `qa-op`/`qa-alice` identities, full binary, Python 3, kubectl, and an administrator-provided `/secure/k8s-admin.yaml` containing Kubernetes context `qa`. Replace paths/context as needed; run examples from the repository root. Converyn `qa-op` and Kubernetes `qa` are separate contexts.

The Kubernetes administrator reviews and applies [rbac.yaml](../../../example/external-kubernetes/rbac.yaml). It creates a dedicated namespace, ServiceAccount, read access to target identity, and ConfigMap get/create/patch access. It leaves the application ConfigMap for Converyn to create. Use fresh demo names to avoid taking over existing business resources. See the [official RBAC documentation](https://kubernetes.io/docs/reference/access-authn-authz/rbac/) for Role/ClusterRole scope.

```bash
kubectl --kubeconfig /secure/k8s-admin.yaml --context qa apply -f example/external-kubernetes/rbac.yaml
```

Generate a dedicated kubeconfig on the operator machine. Only the selected endpoint, CA, and new ServiceAccount token are retained; administrator private keys are excluded. Use a private directory and a new output path. The demo requests a 24-hour token; actual lifetime depends on cluster policy. Rotate all controller copies through a protected channel before expiry; production deployments need an established rotation process.

```bash
install -d -m 0700 /secure/kubernetes
python3 - <<'PYTHON'
import json, os, subprocess
from pathlib import Path
base = ["kubectl", "--kubeconfig", "/secure/k8s-admin.yaml", "--context", "qa"]
source = json.loads(subprocess.check_output(base + ["config", "view", "--minify", "--flatten", "--raw", "-o", "json"]))
cluster = source["clusters"][0]["cluster"]
assert cluster.get("certificate-authority-data"), "an explicit cluster CA is required"
assert not cluster.get("insecure-skip-tls-verify"), "verified TLS is required"
token = subprocess.check_output(base + ["-n", "converyn-demo", "create", "token", "converyn", "--duration=24h"], text=True).strip()
config = {
    "apiVersion": "v1", "kind": "Config",
    "clusters": [{"name": "qa", "cluster": {"server": cluster["server"], "certificate-authority-data": cluster["certificate-authority-data"]}}],
    "users": [{"name": "converyn", "user": {"token": token}}],
    "contexts": [{"name": "qa", "context": {"cluster": "qa", "user": "converyn", "namespace": "converyn-demo"}}],
    "current-context": "qa",
}
os.umask(0o077)
with Path("/secure/kubernetes/qa.json").open("x") as output:
    json.dump(config, output, indent=2)
PYTHON
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get namespace kube-system converyn-demo
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get --raw /version
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa auth can-i create configmaps -n converyn-demo
```

The plugin accepts inline/file CA, client certificates/keys, static `token`, or `tokenFile`. It rejects `exec`, `auth-provider`, insecure TLS, `proxy-url`, and alternate `tls-server-name`; it bypasses proxies. A cloud CLI credential plugin kubeconfig cannot be copied unchanged. Every Converyn controller must reach the endpoint directly; operator connectivity alone is insufficient. See [official kubeconfig concepts](https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/); the narrower Converyn support above applies here.

## 2. Observe target identity and generate an operation

The plugin uses the `kube-system` Namespace UID as physical cluster identity, the business Namespace UID as namespace identity, and reads `/version`. Renaming an endpoint cannot bypass UID checks. Register a new target after a namespace is deleted and recreated. Read identity using the dedicated credentials:

```bash
install -d -m 0700 .local/external-kubernetes
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get namespace kube-system converyn-demo -o json > .local/external-kubernetes/namespaces.json
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get --raw /version > .local/external-kubernetes/version.json
python3 - <<'PYTHON'
import json
from pathlib import Path
root = Path(".local/external-kubernetes")
items = json.loads((root / "namespaces.json").read_text())["items"]
uids = {item["metadata"]["name"]: item["metadata"]["uid"] for item in items}
identity = {"cluster_uid": uids["kube-system"], "namespace_uid": uids["converyn-demo"], "version": json.loads((root / "version.json").read_text())["gitVersion"]}
spec = {"target": {"namespace": "converyn-demo", "identity": identity}, "objects": [{"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "converyn-demo-config", "namespace": "converyn-demo"}, "data": {"message": "hello from Converyn"}}], "timeout_seconds": 60}
(root / "spec.json").write_text(json.dumps(spec, indent=2) + "\n")
PYTHON
converyn -o json compile plugins converyn.kubernetes --action apply --spec .local/external-kubernetes/spec.json > .local/external-kubernetes/operation.json
```

[spec.yaml](../../../example/external-kubernetes/spec.yaml) is the equivalent readable template. `compile plugins` accepts YAML/JSON and only generates an operation: no connection, registration, or publication. The actual binary supplies plugin identity/digest. Compare local `converyn -o json get plugins --local` with controller `converyn --context qa-op -o json get plugins`; relevant binaries must have identical plugin identities.

## 3. Generate workflow, target registration, and connection fragment

The following creates public definitions without kubeconfig/token bytes. It embeds the generated operation unchanged in a workflow and generates its fixed target and controller configuration fragment:

```bash
python3 - <<'PYTHON'
import json
from pathlib import Path
root = Path(".local/external-kubernetes")
operation = json.loads((root / "operation.json").read_text())
plugin = operation["call"]["plugin"]
target = {"uid": "k8s-demo", "revision": 1, "project": "default", "plugin": plugin, "connection": "k8s-qa", "spec": operation["call"]["spec"]["target"]}
workflow = {"uid": "external-k8s-demo", "name": "Publish a Kubernetes ConfigMap", "targets": {"qa": {"external_target_uid": "k8s-demo"}}, "playbook": {"api_version": "converyn/v1", "name": "external-k8s-demo", "steps": [{"id": "publish", "phase": "prepare", "scope": "once_per_run", "operation": operation}]}}
connection = {"plugin_connections": {"k8s-qa": {"identity": plugin, "projects": ["default"], "targets": ["k8s-demo"], "policy": {"kubeconfig": "/var/lib/converyn-controller/kubernetes/qa.json", "context": "qa", "projects": ["default"], "namespaces": ["converyn-demo"], "allow_cluster_resources": False, "allow_privileged_workloads": False}}}}
for name, value in [("target.json", {"action": "target", "target": target}), ("workflow.json", workflow), ("server-fragment.json", connection)]:
    (root / name).write_text(json.dumps(value, indent=2) + "\n")
PYTHON
converyn validate workflows --file .local/external-kubernetes/workflow.json
```

`target.json` is public control-plane registration; `server-fragment.json` is local authorization on every controller; `workflow.json` is the registration entrypoint; `spec.json` generates the operation. Connection name `k8s-qa` must match the target and local config. Target UID `k8s-demo` must match the connection allowlist and workflow preset. `default` is the fixed internal namespace.

## 4. Install private connections on all controllers

Transfer `/secure/kubernetes/qa.json` through a protected channel to every controller. On each controller, assuming the transferred file is `/secure/incoming/qa.json` and the installation guide's service account is `converyn-controller`:

```bash
sudo install -d -o converyn-controller -g converyn-controller -m 0700 /etc/converyn/kubernetes
sudo install -o converyn-controller -g converyn-controller -m 0600 /secure/incoming/qa.json /var/lib/converyn-controller/kubernetes/qa.json
sudo -u converyn-controller kubectl --kubeconfig /var/lib/converyn-controller/kubernetes/qa.json --context qa get namespace kube-system converyn-demo
sudo -u converyn-controller kubectl --kubeconfig /var/lib/converyn-controller/kubernetes/qa.json --context qa auth can-i patch configmaps -n converyn-demo
```

The last two commands are optional diagnostics requiring kubectl; the plugin itself does not. Confirm UIDs match the operator observations. Credential files must be root/service-owned, mode 0600, and not symlinks, with trusted parent directories. Distribute referenced certificate/key files too when using file references.

Back up existing `server.json` and merge `plugin_connections.k8s-qa` from `server-fragment.json`. Preserve member ID, TLS, issuer, state paths, and other connections; never replace the full config with a fragment. Verify allowed target/namespace/plugin identity, restart an idle controller, and check `status`. Every HA member needs the connection grant, readable credentials, and direct API connectivity. Restart one at a time while maintaining quorum. Leader-only configuration fails after failover. Local connections are not replicated through Raft.

## 5. Register, preview, execute, and independently verify

```bash
converyn --context qa-op update targets --file .local/external-kubernetes/target.json
converyn --context qa-op get targets k8s-demo
converyn --context qa-op register workflows --file .local/external-kubernetes/workflow.json
converyn --context qa-alice run workflows external-k8s-demo --target qa --dry-run
converyn --context qa-alice run workflows external-k8s-demo --target qa --wait
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa -n converyn-demo get configmap converyn-demo-config -o yaml
```

Preview should show controller-side `publish`, not a worker. Record the Run ID and inspect `converyn get runs RUN_ID` for status and plugin evidence. The ConfigMap should contain `hello from Converyn` and annotations `converyn.io/owner`, `converyn.io/fence`, and `converyn.io/intent`. Actual execution validates local credentials/grants, connectivity, RBAC, and physical identity; a successful dry run does not.

After changing `spec.json`, repeat the final `compile plugins` command in step 2, generation/validation in step 3, and registration/execution. Retain ownership for the same resources. ExternalTargets support creation of fixed revision=1 identities; business updates do not require registering the same target again. For a changed physical identity/connection, use a new target UID and update local allowlists/presets.

## 6. Extend to Deployments and adjustable parameters

Start with the [Deployment + Service template](../../../example/external-kubernetes/deployment-spec.yaml). Replace actual identity/image digest, run step 2's `compile plugins --spec` against it, then regenerate the workflow using step 3. Add get/create/patch rules for `apiGroups: [apps]` / `resources: [deployments]` and core API `resources: [services]` to the original ConfigMap Role, reviewed/applied by the Kubernetes administrator. The template already declares `spec.replicas` on the first object for the parameter below.

Place full Deployment/Service objects in `spec.json.objects`, then regenerate the operation/workflow. Deployment/StatefulSet/DaemonSet containers and initContainers require reviewed `repository@sha256:...` images, not floating tags. Names must be fixed and namespaces must match. Exclude status, uid, resourceVersion, managedFields, and reserved Converyn annotations.

To expose replicas, add this parameter at workflow top level. The first object must already be a Deployment with `spec.replicas`; this does not apply to the ConfigMap example:

```yaml
parameters:
  replicas:
    kind: integer
    default: 1
    minimum: 1
    maximum: 5
    bindings:
      - step_id: publish
        path: /call/spec/objects/0/spec/replicas
```

```bash
converyn --context qa-alice run workflows external-k8s-demo --target qa --set replicas=2 --dry-run
```

Grant get/create/patch for the workload types before executing; cluster-admin is unnecessary. Deployment readiness checks observedGeneration, updated/available/total replicas, and no unavailable replicas. StatefulSets also check matching current/update revisions; DaemonSets check desired/updated/available counts. ConfigMap/Service/Ingress checks prove intent/ownership, not endpoint or HTTP health; independently verify the application.

## Supported scope and troubleshooting

Supported resources: apps/v1 Deployment, StatefulSet, DaemonSet; v1 Service, ConfigMap, ServiceAccount; policy/v1 PodDisruptionBudget; networking.k8s.io/v1 Ingress; RBAC Role/RoleBinding/ClusterRole/ClusterRoleBinding; apiextensions.k8s.io/v1 CustomResourceDefinition. Cluster resources require explicit `allow_cluster_resources` plus Kubernetes privileges. Supporting CRD definitions does not imply support for arbitrary custom resource instances. Pod, Job, Secret, PVC, and Namespace publication are unsupported; administrators precreate namespaces.

The plugin only provides `apply`: no deletion of removed objects, Helm execution, or arbitrary kubectl commands. Use an explicitly authorized managed-host script or implement plugin capabilities for other needs. Host namespaces, hostPath, privileged containers, and added capabilities are denied by default; OP must explicitly grant `allow_privileged_workloads` and check Kubernetes policy when needed.

| Symptom/error | Action |
| --- | --- |
| Unknown plugin/identity mismatch | Use matching full builds and regenerate calls/grants |
| `local plugin connection missing` | Verify target.connection key in every member's server.json and restart |
| Grant mismatch/scope denied | Match identity, projects, targets, and namespaces |
| Untrusted/unreadable credentials | Check owner, 0600, parents, absolute paths, service-account readability |
| TLS/connection timeout | Direct controller-to-endpoint network, CA, and hostname |
| HTTP 401/403 | Token expiry, client certificates, identity reads, and object RBAC |
| Physical identity changed | Compare both Namespace UIDs; register a new target |
| Ownership/version conflict | Inspect owner, fence, and original Run; do not force takeover |
| Desired state not ready | Inspect workload status/events, image pulls, and scheduling using an administrator identity |

For unknown outcomes, inspect the original Run using [usage](runs.md) instead of blindly retrying with a new request ID. Complete Connection/Spec/Call fields are in [API types](../reference/api-types.md); authorization is covered in [plugins](plugins.md).


With the default installation, place connection credentials under `/var/lib/converyn-controller/kubernetes/`, owned by `converyn-controller`, with directory mode 0700 and file mode 0600. Keep them outside the same host’s root-only Agent directory. Use the actual service account for a custom installation.
