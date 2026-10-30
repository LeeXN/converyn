# 通过主机插件部署

[English](README.md) | [简体中文](README.zh-CN.md)

节点加入后执行已批准的插件操作，无需本机授权文件。

## Sysctl

[workflow.yaml](workflow.yaml) 直接声明 `converyn.linux-host / sysctl`；[sysctl-spec.json](sysctl-spec.json) 在选中的测试节点持久启用 IP 转发。无需另外生成 operation 或 workflow。校验流程，使用匹配的控制器与 Agent 构建进行登记：

```bash
converyn --context qa-op register workflows --file example/plugins
converyn --context qa-alice run workflows host-sysctl-demo --target default --dry-run
converyn --context qa-alice run workflows host-sysctl-demo --target default --wait
```

在节点检查 `sysctl net.ipv4.ip_forward` 及持久配置文件；使用 `get runs RUN_ID` 检查插件回执。

## Docker

修改 [docker-spec.yaml](docker-spec.yaml)：已固定到与 Kubernetes 验证样例相同的 BusyBox 镜像。选择实际平台；更换镜像时固定摘要，并保留 sh/httpd/wget。节点需要 Docker Engine/CLI 和 root Agent 监督进程。[docker.yaml](docker.yaml) 是使用同一节点清单的另一份完整流程。

```bash
converyn --context qa-op register workflows --file example/plugins/docker.yaml
converyn --context qa-alice run workflows docker-web-demo --target default --dry-run
converyn --context qa-alice run workflows docker-web-demo --target default --wait
```

容器通过 `127.0.0.1:8080` 提供服务，须通过健康检查。实际执行时检查容器名、仓库、平台、镜像摘要和控制面任务授权。

## 延伸阅读

[kubeadm-check-input.yaml](kubeadm-check-input.yaml) 是学习 `expand plugins` 的可选展开输入，见[插件指南](../../docs/zh-CN/how-to/plugins.md)；完整安装使用 [Kubernetes 基础环境](../kubernetes/README.zh-CN.md)的流程。[外部 Kubernetes](../external-kubernetes/README.zh-CN.md) 在控制器执行。[plugins/example](../../plugins/example) 是 SDK 开发参考。
