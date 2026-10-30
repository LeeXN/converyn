# 插件使用与开发

[English](../../en/how-to/plugins.md) | [简体中文](plugins.md)

插件通过编译期 feature 链接，不是运行时上传的可执行文件。`full` 包含以下四个插件；实际二进制通过 `get plugins --local` 查看，`get plugins ID` 查看控制器插件详情。所有插件仍经过流程登记、计划、执行授权、回执和审计。

直接部署可从[原生插件示例](../../../example/plugins/README.zh-CN.md)开始，无需拼装流程 JSON。以下生成命令用于说明底层接口。

## 能力清单与执行位置

| 插件 ID | 动作 | 位置/用途 |
| --- | --- | --- |
| `converyn.docker` | `ensure_container` | Node：固定摘要镜像、容器配置和健康检查 |
| `converyn.linux-host` | `sysctl`、`select_kernel` | Node：内核运行参数及通过 grubby 选择已安装内核 |
| `converyn.kubernetes` | `apply` | 控制器：已有外部 Kubernetes API 和就绪证据 |
| `converyn.kubeadm` | init、join、upgrade_control_plane、upgrade_secondary_control_plane、upgrade_node、check、drain、uncordon | 展开为 Node 上的固定 Bash，不接受 `compile plugins` |

## 四个命令分别做什么

| 命令 | 输入 → 输出 | 会不会执行业务变更 |
| --- | --- | --- |
| `get plugins --local` | 当前插件描述；`-o json` 含完整 identity 和摘要 | 只读；明确查看本地二进制，不加载个人 context |
| `compile plugins` | YAML/JSON spec → 含 `kind: plugin` 的 operation | 本地编译，不运行、不登记 |
| `prepare plugins` | 插件准备输入 → 私有材料目录 | 操作机生成私有材料，不是发布；当前 kubeadm 使用 |
| `expand plugins` | 插件输入 → `manifest.json`、`playbook.json`、definitions 文本 | 展开固定脚本，不运行、不登记；当前 kubeadm 使用 |

生成结果需要放入完整流程，`validate workflows`、`register workflows` 后才通过 `run` 执行。输出目录应尚不存在，私有输入不得提交 Git。先核对本地与控制器描述（Node 插件还需核对 Agent 构建）：

```bash
converyn -o json get plugins --local
converyn --context qa-op -o json get plugins
```

## 主机插件：登记与执行

已登记节点信任控制面批准的调用，无需编写插件授权片段。使用[完整插件流程示例](../../../example/plugins/README.zh-CN.md)，填写实际节点后校验、登记、预览和执行。插件身份由匹配构建自动固定；构建不匹配会在运行前报错。

## Docker：固定镜像与集中 Secret

使用[Docker 流程示例](../../../example/plugins/docker.yaml)，固定镜像摘要并选择节点实际平台。容器名、挂载和镜像由已批准的 spec 确定，无需本机插件授权。

私有仓库使用 `credential_ref: {secret: registry-login, version: v1}` 引用集中 Secret。录入文件为 Docker config.json，仅允许 auths 字段，须包含目标仓库；任务使用临时私有文件，结束后清除。不要把密码放进普通 environment。

## kubeadm：准备材料与展开脚本

`prepare` 与 `expand` 是两步。prepare 的 install/upgrade 输入包含 endpoint、CA 等版本相关材料，完整文件与分发路径见[Kubernetes 基础环境示例](../../../example/kubernetes/README.zh-CN.md)。prepare 生成私有材料后，通过受保护通道放到节点；不把 CA 私钥、引导令牌或管理员 kubeconfig 放入流程。

单独理解 expand 可从[check 输入模板](../../../example/plugins/kubeadm-check-input.yaml)开始。替换节点路径、CA_SHA256、NODE_NAME，版本必须是插件支持的 profile，特权 profile `kubernetes` 要由 OP 在节点本地配置。以下只展开，不连接 Kubernetes：

```bash
converyn expand plugins converyn.kubeadm --action check --input example/plugins/kubeadm-check-input.yaml --output .local/plugin-demo/kubeadm-check
python3 - <<'PYTHON'
import json
from pathlib import Path
root = Path(".local/plugin-demo/kubeadm-check")
expanded = json.loads((root / "manifest.json").read_text())
workflow = {"uid": "kubeadm-check-demo", "name": "Check existing kubeadm cluster", "inventory": {"nodes": [{"uid": "node-a", "address": "192.0.2.1", "hostname": "node-a"}]}, "targets": {"qa": {"node_group_uid": "example-hosts", "hosts": "all"}}, "playbook": {"api_version": "converyn/v1", "name": "kubeadm-check-demo", "steps": expanded["steps"]}, "definitions": expanded["definitions"]}
(root / "workflow.json").write_text(json.dumps(workflow, indent=2) + "\n")
PYTHON
converyn validate workflows --file .local/plugin-demo/kubeadm-check/workflow.json
```

`playbook.json` 只有步骤，不是完整流程。`manifest.json.definitions` 已按摘要内联全部展开脚本，所以生成的 workflow 不再需要 `scripts` 映射。审核步骤选择器和 origin、profile、固定环境，确认本机路径和权限后，再 register、dry-run、run。init/join/upgrade 等动作还要求 `BINARY_SHA256`、`CONFIG_PATH`、`CONFIG_SHA256`；辅助脚本已合并进每个定义，无需在节点复制 common.sh。

## 外部插件与参数边界

Kubernetes 在控制器执行，不使用 Node 的 plugins grant。它需要控制器 `server.json.plugin_connections`、私有 kubeconfig、已登记 ExternalTarget 和流程 `external_target_uid`；完整路径见[连接已有 Kubernetes](external-kubernetes.md)。混合流程同时声明 inventory、`node_group_uid`、`hosts` 与 `external_target_uid`。

业务参数通过 `parameters.bindings` 绑定 spec 的允许字段，再重新验证完整调用。不能用参数改变目标、插件身份、凭据引用、所有者或资源身份。`compile plugins`、工作流登记、dry-run 与真实授权是不同阶段；生成成功不代表获准执行。

## 插件开发与恢复契约

插件 SDK 和示例入口分别在 [plugin-api](../../../crates/plugin-api) 与 [example 插件](../../../plugins/example)。实现描述、编译、验证、本机授权、执行和证据校验；脚本包实现 prepare/expand。新插件必须由 runtime 注册并通过 feature 编入；不能仅复制一个 .so 到服务器。

validate_target、bind_target、validate_call 等纯契约的结果在共识前记录到 Journal；副本重放固定结果。纯契约不能进行外部 I/O，实际请求由 Node 或控制器执行。未知结果保留原授权与回执；可收敛操作重新观察，其他操作需管理员核实，不可盲目重放。

完整脚本权限见[流程编写](workflows.md)，数据结构见[API 类型](../reference/api-types.md)，命令选项见[CLI](../reference/cli.md)。



目录准备使用通用的绝对规范路径，平台不内置业务目录白名单。例如在原生流程中声明：

```yaml
- id: prepare-data
  phase: rollout
  scope: per_node
  hosts: application
  operation:
    kind: plugin
    id: converyn.linux-host
    action: directories
    spec:
      paths: [/srv/my-service/data]
```

已登记节点按流程执行主机操作，无需节点授权片段。
