# 连接已有的外部 Kubernetes

[English](../../en/how-to/external-kubernetes.md) | [简体中文](external-kubernetes.md)

本教程从已有 Kubernetes 集群发布一个 ConfigMap，再说明如何改为 Deployment。只使用外部 API 时不需要在 Kubernetes 节点安装 Converyn Agent，也不需要登记主机 Cluster。Converyn 控制器直接访问 API Server；操作机上的 `kubectl` 仅用于准备和独立验证。

若要从空白 Linux 主机安装 Kubernetes，走[基础环境示例](../../../example/kubernetes/README.zh-CN.md)的 kubeadm 路径；它与接入已有集群是两个任务。

完整的原生 YAML 流程见[可部署示例](../../../example/external-kubernetes/README.zh-CN.md)。下面的凭据准备同样适用于该路径。

## 1. 准备连接、身份和权限

前置条件：Converyn 控制面及 `qa-op`/`qa-alice` 身份已按[安装指南](installation.md)准备，操作机有 full 二进制、Python 3 和 kubectl，外部集群管理员提供 `/secure/k8s-admin.yaml`（其中 context 为 `qa`）。替换路径和 context，所有公开示例从仓库根目录执行。Converyn context `qa-op` 与 Kubernetes context `qa` 分别管理。

先审阅并由 Kubernetes 管理员应用 [rbac.yaml](../../../example/external-kubernetes/rbac.yaml)。它创建专用命名空间、ServiceAccount、只读目标身份权限和 ConfigMap 的 get/create/patch 权限；不会创建示例 ConfigMap（留给 Converyn 管理）。使用新的示例名称，避免接管已有业务资源。RBAC 的 Role/ClusterRole 范围见 [Kubernetes 官方说明](https://kubernetes.io/docs/reference/access-authn-authz/rbac/)。

```bash
kubectl --kubeconfig /secure/k8s-admin.yaml --context qa apply -f example/external-kubernetes/rbac.yaml
```

用下列 Python 在操作机生成专用 kubeconfig：只保留所选 API Server、CA 和新 ServiceAccount 令牌，不复制管理员私钥。输出文件必须不存在，目录为私有目录。示例请求 24 小时令牌，实际有效期受集群策略限制；到期前通过受保护通道轮换所有控制器上的副本。长期部署应接入自己的凭据轮换流程。

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

当前插件支持 kubeconfig 内嵌/文件 CA、客户端证书和私钥、静态 `token` 或 `tokenFile`；拒绝 `exec`、`auth-provider`、跳过 TLS 校验、`proxy-url` 及不同于 endpoint 主机名的 `tls-server-name`，不使用代理。不要直接复制依赖云 CLI 登录插件的 kubeconfig。API Server 地址必须从每个 Converyn 控制器可达；操作机能连不证明控制器能连。kubeconfig 的通用概念见[官方说明](https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/)，Converyn 支持范围以本段为准。

## 2. 读取实际目标身份，生成插件操作

插件把 `kube-system` Namespace UID 当作物理集群身份，把业务 Namespace UID 当作命名空间身份，并读取 `/version`。只改 endpoint 或名字不能绕过 UID 检查；Namespace 删除重建后应按新身份登记新目标。用专用 kubeconfig 读取真实身份：

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

[spec.yaml](../../../example/external-kubernetes/spec.yaml) 是相同 spec 的可读模板。`compile plugins` 接受 YAML/JSON，只生成 operation，不连接集群、不登记、不发布；返回的插件身份和摘要由实际二进制生成，不能手填。用 `converyn -o json get plugins --local` 查看本地能力，用 `converyn --context qa-op -o json get plugins` 核对控制器能力；所有相关二进制的插件身份必须一致。

## 3. 生成流程、目标登记文件和本机连接片段

下面仅生成公开业务定义，不包含 kubeconfig 或令牌。它把生成的 operation 原样嵌入完整流程，同时生成固定目标和控制器配置片段：

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

`target.json` 是控制面公开登记内容，`server-fragment.json` 是每个控制器的本机授权，`workflow.json` 是流程入口，`spec.json` 是生成插件调用的输入。连接名 `k8s-qa` 在 target 和本机片段中必须一致，目标 UID `k8s-demo` 在连接允许列表与流程预设中必须一致。`project: default` 是固定内部命名空间。

## 4. 在所有控制器安装私有连接

通过私有通道将 `/secure/kubernetes/qa.json` 送到每个控制器。以下在控制器上执行，假设传来的私有文件为 `/secure/incoming/qa.json`，服务账号沿用安装指南的 `converyn-controller`：

```bash
sudo install -d -o converyn-controller -g converyn-controller -m 0700 /etc/converyn/kubernetes
sudo install -o converyn-controller -g converyn-controller -m 0600 /secure/incoming/qa.json /var/lib/converyn-controller/kubernetes/qa.json
sudo -u converyn-controller kubectl --kubeconfig /var/lib/converyn-controller/kubernetes/qa.json --context qa get namespace kube-system converyn-demo
sudo -u converyn-controller kubectl --kubeconfig /var/lib/converyn-controller/kubernetes/qa.json --context qa auth can-i patch configmaps -n converyn-demo
```

最后两条是控制器上的可选诊断（需要 kubectl）；插件自身不依赖 kubectl。核对 UID 与操作机读取的一致。凭据文件应由 root 或服务账号所有、0600、不能是符号链接，父目录不能被不可信用户写入。引用外部证书/私钥时，也要分发所引用文件并满足权限要求。

备份已有 `server.json`，把生成的 `server-fragment.json` 中 `plugin_connections.k8s-qa` 合并进去。保留原成员 ID、TLS、issuer、数据目录及其他连接，不要用片段覆盖完整配置。确认该连接允许的 target、namespace 与插件身份后，空闲时重启 `converyn-controller`，运行 `status` 确认控制面可用。HA 各成员都需要相同连接名和授权、各自可读的凭据及 API 网络；逐台重启并保持多数派。只在当前 Leader 配置会造成切换后失败。本机连接不是通过 Raft 自动分发的。

## 5. 登记、预览、执行与独立核实

```bash
converyn --context qa-op update targets --file .local/external-kubernetes/target.json
converyn --context qa-op get targets k8s-demo
converyn --context qa-op register workflows --file .local/external-kubernetes/workflow.json
converyn --context qa-alice run workflows external-k8s-demo --target qa --dry-run
converyn --context qa-alice run workflows external-k8s-demo --target qa --wait
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa -n converyn-demo get configmap converyn-demo-config -o yaml
```

预览应显示控制器位置的 `publish`，而不是某台 worker。记录返回的 Run ID，再 `converyn get runs RUN_ID` 查看状态和插件回执；ConfigMap 内容应为 `hello from Converyn`，metadata 含 `converyn.io/owner`、`converyn.io/fence`、`converyn.io/intent`。只有真实执行才验证 kubeconfig、本机授权、网络、外部权限与物理身份；dry-run 成功不足以证明这些条件。

修改 `spec.json` 内容后重新执行第 2 步最后的 `compile plugins`、第 3 步生成及 validate，再 register/run；同一个资源保持原流程/预设所有者。ExternalTarget 只支持创建固定的 revision=1 目标；业务更新不需要重新登记相同目标。换物理身份或连接时使用新的目标 UID，并相应更新本机允许列表、流程预设。

## 6. 改为 Deployment 与可调参数

可直接从 [Deployment + Service 模板](../../../example/external-kubernetes/deployment-spec.yaml)改写：替换实际目标身份、镜像摘要后，对它执行第 2 步的 `compile plugins --spec`，再按第 3 步重新生成流程。原 ConfigMap Role 的 rules 需要额外加入 `apiGroups: [apps]` / `resources: [deployments]`，以及核心 API 的 `resources: [services]`，两者均授予 get/create/patch；由 Kubernetes 管理员审核并应用。模板在第一个对象声明 `spec.replicas`，可直接使用下面的参数绑定。

在 `spec.json.objects` 中放完整 Deployment/Service 等对象，重新生成 operation 和 workflow。Deployment/StatefulSet/DaemonSet 的容器与 initContainers 镜像必须是实际审核的 `repository@sha256:...`，不接受浮动 tag。各对象写固定 name，namespace 与 target 一致，不能带 status、uid、resourceVersion、managedFields 或保留的 Converyn 注解。

想让执行者改副本数，在生成的流程顶层增加以下参数，且第一个对象必须已经是具有 `spec.replicas` 的 Deployment；不能直接用于上面的 ConfigMap 示例：

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

给专用账号增加该业务对象的 get/create/patch 权限后再执行；不要因此授予 cluster-admin。Deployment 的成功依据 observedGeneration、updated/available/total replicas 与无 unavailable；StatefulSet 还检查 current/update revision，DaemonSet 检查 desired/updated/available。ConfigMap、Service、Ingress 等仅确认对象意图与所有权，不代表端口或 HTTP 业务健康，应另外验证业务接口。

## 支持范围与排错

当前原生 API 插件支持：apps/v1 Deployment、StatefulSet、DaemonSet；v1 Service、ConfigMap、ServiceAccount；policy/v1 PodDisruptionBudget；networking.k8s.io/v1 Ingress；RBAC Role/RoleBinding/ClusterRole/ClusterRoleBinding；apiextensions.k8s.io/v1 CustomResourceDefinition。集群级资源默认拒绝，需 `allow_cluster_resources` 和对应 Kubernetes 权限。CRD 定义受支持，不表示任意自定义资源实例都受支持。Pod、Job、Secret、PVC、Namespace 的发布不在当前列表；Namespace 由管理员预创建。

此插件仅有 `apply`，不会自动删除清单中移除的对象，不执行 Helm，不提供任意 kubectl 命令。需要 Helm 或其他资源类型时，明确使用受管主机脚本及其授权，或开发新的插件能力。默认拒绝 hostNetwork/hostPID/hostIPC、hostPath、特权容器及额外 capabilities，确需使用时由 OP 配置 `allow_privileged_workloads` 并核对 Kubernetes 策略。

| 现象/错误 | 处理 |
| --- | --- |
| `unknown plugin` / 身份不匹配 | 本地、控制器用相同 full 构建；重新生成固定调用与授权 |
| `local plugin connection missing` | 每个成员的 server.json 是否包含 target.connection 对应的键，是否已重启 |
| `connection grant mismatch` / `scope denied` | identity、projects、targets、namespaces 是否全部匹配 |
| 凭据不可信或无法读取 | 文件属主、0600、父目录和绝对路径；服务账号可读 |
| TLS/连接超时 | 每个控制器到 kubeconfig endpoint 的直接网络、CA 与证书主机名 |
| HTTP 401 / 403 | 令牌有效期、客户端证书、Namespace 身份读取及业务对象 RBAC |
| `physical Kubernetes target identity changed` | 比较 kube-system/业务 Namespace UID，重新登记新目标而非绕过检查 |
| 所有权/版本冲突 | 核查现有 owner、fence 和原 Run，禁止强制接管其他工具的资源 |
| desired state 未就绪 | 用管理员身份查 Deployment/StatefulSet/DaemonSet 状态、事件、镜像拉取与调度 |

回执未知时按[使用指南](runs.md)核实原 Run，不用新请求 ID 盲重试。完整连接、spec 和 Call 字段见 [API 类型](../reference/api-types.md)，能力授权见[插件指南](plugins.md)。


默认安装中，将连接凭据放到控制器服务账号可访问的 `/var/lib/converyn-controller/kubernetes/`，目录 0700、文件 0600，归属 `converyn-controller`。不要放进同机 root 专用的 Agent 目录。自定义服务账号时按实际账号设置所有权。
