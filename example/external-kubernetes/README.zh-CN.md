# 发布到已有 Kubernetes 集群

[English](README.md) | [简体中文](README.zh-CN.md)

直接登记 [workflow.yaml](workflow.yaml)，通过 Kubernetes API 发布 ConfigMap。流程由控制器执行，Kubernetes 节点无需安装 Converyn Agent，也无需主机 Cluster。使用插件一致的 `full` 构建，以及具备目标、流程登记和执行权限的个人上下文。

## 准备连接

按[连接和专用凭据指南](../../docs/zh-CN/how-to/external-kubernetes.md)审核并应用 [rbac.yaml](rbac.yaml)，创建专用 ServiceAccount 的私有 kubeconfig，并安装到**每台控制器**。控制器须能直连 API。不要把凭据打包进流程。

使用专用凭据读取真实目标身份：

```bash
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get namespace kube-system -o jsonpath='{.metadata.uid}{"\n"}'
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get namespace converyn-demo -o jsonpath='{.metadata.uid}{"\n"}'
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa get --raw /version
```

替换 [spec.yaml](spec.yaml) 中全部三个身份占位符。物理集群 UID 是 `kube-system` Namespace 的 UID。按需修改 ConfigMap 数据。[connection-policy.json](connection-policy.json) 固定控制器本地 kubeconfig 路径、context 和命名空间授权；按实际环境调整。

## 准备公开登记信息

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

审核 `target.json`，将 `server-fragment.json` 合并到每台控制器的现有配置。保留成员身份、TLS、状态和其他连接。私有 kubeconfig 单独安装，空闲时逐台重启控制器并检查状态。这些命令只生成公开的目标及授权信息，流程本身已经完整。

## 部署与验证

```bash
converyn --context qa-op update targets --file .local/external-kubernetes/target.json
converyn --context qa-op register workflows --file example/external-kubernetes
converyn --context qa-alice run workflows external-k8s-demo --target qa --dry-run
converyn --context qa-alice run workflows external-k8s-demo --target qa --wait
kubectl --kubeconfig /secure/kubernetes/qa.json --context qa -n converyn-demo get configmap converyn-demo-config -o yaml
```

使用 `converyn get runs RUN_ID` 检查控制器插件证据。实际 ConfigMap 应含消息和 `converyn.io/owner`、`converyn.io/fence`、`converyn.io/intent` 注解。后续改动只需修改 `spec.yaml`、重新登记并运行流程，无需重建不变的目标。命名空间重建或切换物理集群时，使用新目标 UID 并同步授权。

## 部署应用

先应用专用的[应用 RBAC](application-rbac.yaml)，用于发布、观察 rollout 和服务端口转发。

[deployment.yaml](deployment.yaml) 引用 [deployment-spec.yaml](deployment-spec.yaml)，包含 Deployment、Service 和有边界的副本数参数。将观测到的身份复制到 spec，核对固定的 BusyBox 镜像，先为专用 Role 增加 `apps/deployments` 和核心 `services` 的 get/create/patch 权限，再执行：

```bash
kubectl --kubeconfig /secure/k8s-admin.yaml --context qa apply -f example/external-kubernetes/application-rbac.yaml
converyn validate workflows --file example/external-kubernetes/deployment.yaml
converyn --context qa-op register workflows --file example/external-kubernetes/deployment.yaml
converyn --context qa-alice run workflows external-k8s-web-demo --target qa --set replicas=2 --dry-run
converyn --context qa-alice run workflows external-k8s-web-demo --target qa --set replicas=2 --wait
```

使用准备好的 kubeconfig 执行 `kubectl --kubeconfig /secure/kubernetes/qa.json --context qa rollout status deployment/converyn-demo-web -n converyn-demo` 验证工作负载就绪。两个流程共用目标 `k8s-demo` 和连接 `k8s-qa`。安装 Kubernetes 本身见[基础环境部署](../kubernetes/README.zh-CN.md)。

## 升级组件与回退

`image` 只接受两个已固定摘要的 BusyBox 版本，`release_version` 同时控制 HTTP 返回的应用版本。这样升级会真实更换容器镜像，而非只改变副本数。下面命令先发布 v1，再升级 BusyBox 并发布 v2，最后回退。

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

通过 `kubectl --kubeconfig /secure/kubernetes/qa.json --context qa port-forward -n converyn-demo service/converyn-demo-web 18082:80` 建立本地连接，再用另一个终端 `curl --fail http://127.0.0.1:18082/` 验证实际响应为本次选择的 v1/v2；同时核对 Run 中的资源与就绪证据。CI 样例见 [CI 集成](../ci/README.zh-CN.md)。

集群节点必须能下载这两个固定摘要，或预装包含原始摘要的审核过 OCI 归档。仅缓存相同 tag、重新打包后摘要不同的镜像不够。镜像拉取失败时先修复来源或补齐对应摘要，再核查原 Run 和工作负载；`needs_attention` 按[任务恢复](../../docs/zh-CN/how-to/runs.md)处理。
