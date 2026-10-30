# Linux 应用持续交付

[English](README.md) | [简体中文](README.zh-CN.md)


向已登记且具备 Python 3、systemd 的 Linux 节点交付 HTTP 应用及服务单元。按[快速开始](../../docs/zh-CN/tutorials/quickstart.md)修改 inventory、准备节点组并授予执行者权限。流程自行创建演示目录，无需修改 Agent 白名单；HTTP 服务以 nobody 身份运行，审核后的发布脚本显式声明主机执行；流程显式重启服务以加载交付的应用代码。

## 发布、升级与回退

```bash
converyn validate workflows --file example/node-demo
converyn --context qa-op register workflows --file example/node-demo
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v1 --dry-run
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v1 --wait
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v2 --wait
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v1 --wait
converyn --context qa-alice logs runs RUN_ID --step verify
```

## 验收与清理

每次验证都检查回环端口 18080 上的 HTTP 健康、版本与内容 SHA-256。`publish.sh` 保留不可变的 `data/releases/VERSION`，通过原子切换 `data/current` 发布；已有版本内容冲突时拒绝覆盖。回退 v1 后保留 v2 供检查。

停止使用后，停止并禁用 `converyn-node-demo.service`，移除单元并重新加载 systemd；确认不再需要发布历史后再删除 `/opt/converyn-node-demo`。保留 Converyn 身份与状态。流水线模板见[CI 集成](../ci/README.zh-CN.md)。
