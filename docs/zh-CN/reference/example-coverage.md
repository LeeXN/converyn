# 示例能力覆盖

| 能力 | 入口 | 验收结果 |
| --- | --- | --- |
| 文件、制品、软件包、服务、HTTP、脚本、插件 | [showcase](../../../example/showcase/README.zh-CN.md) | 受管字节、精确包版本及服务健康 |
| Secret 与类型化输出 | [outputs](../../../example/outputs/README.zh-CN.md) | 公开结果不含明文，消费步骤取得正确输入 |
| 有序 inventory 与分批 | [workflow-directory](../../../example/workflow-directory/README.zh-CN.md) | 顺序正确，流程标签独立 |
| Linux 发布与回退 | [node-demo](../../../example/node-demo/README.zh-CN.md) | v1 → v2 → v1，HTTP 摘要一致 |
| Kubernetes 与社区存储 | [kubernetes](../../../example/kubernetes/README.zh-CN.md) | DNS、网络、PVC、TLS、OCI 和指标 |
| 外部 Kubernetes CD | [external-kubernetes](../../../example/external-kubernetes/README.zh-CN.md) | Deployment 就绪且服务返回期望版本 |
| 漂移修复 | [reconciliation](../../../example/reconciliation/README.zh-CN.md) | 新的系统 Run 恢复受管内容 |
| 重启 | [maintenance](../../../example/maintenance/README.zh-CN.md) | 新 boot ID 与认证恢复 |
| CI 集成 | [ci](../../../example/ci/README.zh-CN.md) | 可审查模板，本仓库不触发部署 |

[身份与权限](../how-to/access.md)、[节点组](../how-to/node-groups.md)、[证书](../how-to/certificates.md)、[Web](../how-to/web-console.md)、[任务控制与恢复](../how-to/runs.md)通过专门操作教程验收。综合示例不自动重启主机，CI 模板也不创建平台或发放凭据。
