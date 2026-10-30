# 最小流程

[English](README.md) | [简体中文](README.zh-CN.md)

向已登记节点写入一个配置文件，再验证文件。完成[安装和节点登记](../../docs/zh-CN/tutorials/quickstart.md)后，从这个样例开始。所有命令从仓库根目录执行；`qa-op` 登记流程，`qa-alice` 需要 `demo` 的执行权限。

## 准备

修改 [inventory.yaml](inventory.yaml)，填写实际 Node UID、地址和主机名。流程使用自己的节点清单，需要先创建绑定的[平台节点组](../../docs/zh-CN/how-to/node-groups.md)。UID 改变时，也要修改 [workflow.yaml](workflow.yaml) 中的 `node_values` 键。

节点加入后即可执行已批准的原生文件操作，无需编辑 Agent 权限。

## 部署

```bash
converyn validate workflows --file example/workflow
converyn --context qa-op register workflows --file example/workflow
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --dry-run
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --wait
```

## 阅读与修改

| 文件 | 学习内容 |
| --- | --- |
| [workflow.yaml](workflow.yaml) | 目标、文件交付、验证步骤和节点默认值 |
| [values.yaml](values.yaml) | 默认变量；`VERSION` 可逐次覆盖，`REGION` 不可修改 |
| [config.txt](config.txt) | 交付到 `/opt/converyn-demo/config` 的固定内容 |
| [verify.sh](verify.sh) | 验证实际文件，打印解析后的变量 |

改变 `VERSION` 不会改变文件内容：变量通过环境传给脚本。修改源文件后重新登记。使用 `converyn get runs RUN_ID` 和 `converyn logs runs RUN_ID --step verify` 查看结果，两个操作都应有成功回执。接着阅读[有序分组](../workflow-directory/README.zh-CN.md)或[站点发布](../node-demo/README.zh-CN.md)。
