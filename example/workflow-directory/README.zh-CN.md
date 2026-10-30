# 有序节点分组

[English](README.md) | [简体中文](README.zh-CN.md)

按指定顺序在两个已登记节点运行简单检查脚本。先完成[最小流程](../workflow/README.zh-CN.md)；本例只需普通账户执行，无需业务目录写授权。

## 准备

修改 [inventory.yaml](inventory.yaml) 中的 UID、地址和主机名。`database: [node-b, node-a]` 决定执行顺序；`monitoring` 演示同一节点可以有多个标签。修改 [values.yaml](values.yaml) 可改变输出消息。

## 部署

```bash
converyn validate workflows --file example/workflow-directory
converyn --context qa-op register workflows --file example/workflow-directory
converyn --context qa-alice run workflows ordered-health --target default --dry-run
converyn --context qa-alice run workflows ordered-health --target default --wait
```

## 阅读与修改

[workflow.yaml](workflow.yaml) 使用 `database` 和 `max_parallel: 1`：node-b 完成后才执行 node-a。[scripts/check.sh](scripts/check.sh) 通过 `CONVERYN_PACKAGE_ROOT` 读取已登记依赖包中的 [lib/message.sh](lib/message.sh)。登记时会打包文件，无需另外复制到 Agent。

使用 `converyn logs runs RUN_ID --step check` 查看两个节点的消息。将分组改为 `database[0]` 可只选择第一个节点。选择器、依赖包限制和权限见[目录语义](../../docs/zh-CN/how-to/workflow-directories.md)。
