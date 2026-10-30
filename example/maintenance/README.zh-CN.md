# 计划重启维护

[English](README.md) | [简体中文](README.zh-CN.md)


此流程会真实重启所选主机。只用于测试主机或已批准的维护窗口；先修改 inventory、准备平台节点组并结束冲突业务。每次处理一台主机，成功要求新的 boot ID、Agent 恢复及验证回执成功。

## 运行

```bash
converyn validate workflows --file example/maintenance
converyn --context qa-op register workflows --file example/maintenance
converyn --context qa-op run workflows maintenance-reboot --target default --dry-run
converyn --context qa-op run workflows maintenance-reboot --target default --request-id maintenance-001 --wait --timeout-seconds 900
converyn --context qa-op get runs run-maintenance-001
converyn --context qa-op logs runs run-maintenance-001
```

## 恢复

超时不代表主机没有重启。检查原 Run、控制台和 Agent 心跳；网络错误恢复沿用原请求 ID，不通过再次重启掩盖未知结果。此示例不写业务数据。详见[任务控制](../../docs/zh-CN/how-to/runs.md)。
