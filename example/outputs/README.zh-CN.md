# 步骤输出

[English](README.md) | [简体中文](README.zh-CN.md)

第一个节点生成消息和随机秘密令牌，所有选中节点消费已提交的输出。先完成[最小流程](../workflow/README.zh-CN.md)。

## 准备

修改 [inventory.yaml](inventory.yaml)，填入已登记节点。普通账户执行即可。控制器需要 `admin init` 创建的私有 `output_keyring`；每台控制器使用同一份密钥环。

## 部署

```bash
converyn validate workflows --file example/outputs
converyn --context qa-op register workflows --file example/outputs
converyn --context qa-alice run workflows outputs-demo --target default --dry-run
converyn --context qa-alice run workflows outputs-demo --target default --wait
converyn --context qa-alice get runs RUN_ID
converyn --context qa-alice logs runs RUN_ID --step consume
```

## 阅读与修改

[workflow.yaml](workflow.yaml) 声明输出类型、令牌一小时有效期和消费绑定。[produce.sh](produce.sh) 通过已安装 Converyn 的 `task emit` 发布结果；[consume.sh](consume.sh) 验证收到的消息和令牌。增加节点即可观察一对多分发。

将 `RUN_ID` 替换为返回的运行 ID。详情展示公开消息和受保护输出标记。生产步骤不保留日志，消费日志中的已知秘密值会被遮盖。汇集和恢复语义见[输出契约](../../docs/zh-CN/how-to/outputs.md)。
