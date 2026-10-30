# 可运行示例

[English](README.md) | [简体中文](README.zh-CN.md)

先完成[安装教程](../docs/zh-CN/tutorials/quickstart.md)，再选择示例。修改真实 Node UID、地址和主机名，平台节点组须包含目标节点；外部 Kubernetes 使用独立目标及连接。

| 示例 | 展示内容 |
| --- | --- |
| [最小任务](workflow/README.zh-CN.md) | 文件、脚本、变量 |
| [功能综合展示](showcase/README.zh-CN.md) | 软件包、制品、服务、检查、输出、Secret |
| [完整 Kubernetes](kubernetes/README.zh-CN.md) | 社区集群、基础服务与卸载 |
| [Linux CD](node-demo/README.zh-CN.md) | v1 → v2 → 验证 → v1 |
| [Kubernetes CD](external-kubernetes/README.zh-CN.md) | 外部 API 连接与应用版本发布 |
| [持续核对](reconciliation/README.zh-CN.md) | 检测与修复文件漂移 |
| [维护](maintenance/README.zh-CN.md) | 有序重启与恢复证据 |
| [流程目录](workflow-directory/README.zh-CN.md) | 有序分组与共享脚本 |
| [步骤输出](outputs/README.zh-CN.md) | 受保护的类型化数据流 |
| [插件](plugins/README.zh-CN.md) | 主机与容器插件 |
| [CI 模板](ci/README.zh-CN.md) | GitHub Actions、GitLab CI、Jenkins |

## 统一使用方式

Kubernetes 和综合示例先按自身 README 准备源目录；其他示例可直接登记。

```bash
converyn validate workflows --file example/workflow
converyn --context qa-op register workflows --file example/workflow
converyn --context qa-alice run workflows demo --target qa --dry-run
converyn --context qa-alice run workflows demo --target qa --wait
```

登记固定源文件快照；修改后重新登记。验收检查 Run 回执及业务结果，网络重试沿用请求 ID。私有输入和生成材料放在忽略的 `.local/`。完整覆盖表见[能力参考](../docs/zh-CN/reference/example-coverage.md)。
