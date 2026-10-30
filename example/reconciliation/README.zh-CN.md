# 持续核对与漂移修复

[English](README.md) | [简体中文](README.zh-CN.md)


按[快速开始](../../docs/zh-CN/tutorials/quickstart.md)修改 inventory 并准备平台节点组。首次成功 Run 固定文件期望内容，之后每 30 秒核对该快照并修复漂移；通过关联的系统 Run 和回执查看结果。

## 运行并制造漂移

```bash
converyn validate workflows --file example/reconciliation
converyn --context qa-op register workflows --file example/reconciliation
converyn --context qa-op run workflows reconciliation-demo --target default --wait
# On an enrolled target host, intentionally change only the demonstration file:
sudo sh -c 'printf "changed\n" > /opt/converyn-reconciliation/config.txt'
# After one reconciliation period, inspect the host and the associated system Run:
cat /opt/converyn-reconciliation/config.txt
converyn --context qa-op get workflows reconciliation-demo
converyn --context qa-op get runs
```

## 验收与停止

文件应恢复为 `managed configuration`，对应修复 Run 成功。改作其他用途前，取消创建该代次的原始 Run 来停止持续核对。原成功回执保留；已授权的核对任务可能先完成。仅设置 `reconcile_seconds: null` 或停用流程，不会终止此前冻结代次。原 Run ID 来自首次运行结果。

只有 file、binary、service、HTTP 检查及可收敛插件参与自动核对。脚本、软件包、重启和主机一次性钩子不会自动重放。详见[流程语义](../../docs/zh-CN/how-to/workflows.md)。

```bash
converyn --context qa-op cancel runs ORIGINAL_RUN_ID
converyn --context qa-op get workflows reconciliation-demo
```
