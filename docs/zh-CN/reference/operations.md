# 内置功能参考

[English](../../en/reference/operations.md) | [简体中文](operations.md)

本页展示流程 `playbook.steps[].operation` 的全部内置类型。片段要放进[完整流程](../how-to/workflows.md)，不是独立可登记的文件。主机操作由目标 Agent 执行；父目录和工具由 OP 预先准备；节点按控制面批准的任务执行。插件另见[插件指南](../how-to/plugins.md)。

## 选择哪种操作

| kind | 解决什么问题 | 执行前配置/依赖 |
| --- | --- | --- |
| `file` | 按固定文本和摘要写文件、校验权限 | 已批准的主机操作；无需节点白名单 |
| `binary` | 下载固定摘要制品、发布到指定路径 | 控制器制品，或外部 `artifact_sources` |
| `package` | 从固定制品安装精确版本 deb/rpm | 已批准的主机操作；无需节点白名单 |
| `service` | 确保 systemd 服务运行/停止及开机启用状态 | 已批准的主机操作；无需节点白名单 |
| `http_check` | 从目标节点校验 HTTP 状态和可选响应摘要 | 已批准的主机操作；无需节点白名单 |
| `exec` | 自定义 Bash 脚本 | 默认低权限；主机改动声明 `execution: host` |
| `reboot` | 重启并等待新的 boot ID 与恢复证据 | 已批准的主机操作；无需节点白名单 |
| `plugin` | 调用编入的扩展能力 | 固定 plugin call；已批准的节点操作或控制器 connection |

已登记节点执行控制面批准的操作，无需逐台授权。仅在使用外部文件服务器时配置 `artifact_sources`；由控制器分发的制品使用节点身份。添加外部来源时保留已有 Agent 身份、TLS 与状态：
```json
{
  "artifact_sources": {
    "sources": {
      "releases": {
        "origin": "https://artifacts.example.test",
        "ca_file": "/etc/converyn/artifact-ca.crt",
        "bearer_token_file": "/etc/converyn/artifact-token"
      }
    }
  }
}
```

来源必须是 HTTPS origin；仅当使用系统 CA 或无需鉴权时省略对应字段。私钥、令牌、Secret 文件在执行端保护。修改本机配置后，空闲时重启 Agent 并确认心跳。

## file：写业务文本

在流程顶层写 `files: {config: config.json}`，把以下步骤放进 `playbook.steps`。打包器计算 `sha256`，无需手算。目标父目录 `/opt/myapp` 必须存在。模式用十进制整数：420 = 0644，384 = 0600，493 = 0755。内容按原字节发布，不替换变量，不创建 systemd 服务或触发重启。

```yaml
- id: config
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: file
    path: /opt/myapp/config.json
    mode: 420
```

## binary：发布制品

`sha256` 和 `size` 来自审核后的实际制品，下面的摘要与长度是占位值，必须替换。操作机可用 `sha256sum app` 和 `wc -c < app` 获取。来源名 `releases` 对应每个 Agent 的本地配置，最终下载地址为 origin 加 path。制品不是通过 `files` 内联的；此操作不会自动解压 tar/zip。可选 `version_directory` 按摘要和模式保留版本并切换发布路径，需预先准备版本目录。

```yaml
- id: publish
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: binary
    path: /opt/myapp/bin/app
    version_directory: /opt/myapp/versions
    mode: 493
    artifact:
      sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      size: 12345
      locations:
        - source: releases
          path: /myapp/app-linux-amd64
```

## package：安装精确版本

将下面的包名、版本、摘要、字节数和来源路径一起替换为真实 deb。`manager` 为 `deb` 或 `rpm`，Agent 上需安装相应工具。它安装所指制品，不接受 `latest` 或隐式从任意源下载包。RPM 多版本包可加 `install_only: true`，deb 不支持。包管理脚本可能有副作用，不要把未知结果当成可无条件重放。

```yaml
- id: install
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: package
    manager: deb
    name: myapp
    version: "1.2.3-1"
    artifact:
      sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      size: 12345
      locations:
        - source: releases
          path: /myapp/myapp_1.2.3-1_amd64.deb
    timeout_seconds: 300
```

## service：确保服务状态

服务单元须已安装。此操作确认运行和开机启用状态；更新单元文件后，按应用需要显式安排重新加载或重启。

```yaml
- id: start
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: service
    name: myapp.service
    running: true
    enabled: true
```

## http_check：验证实际 HTTP 结果

从执行节点请求流程声明的 URL，确认响应状态；可选响应摘要检查用于确认实际内容。

```yaml
- id: health
  phase: verify
  scope: per_node
  hosts: worker
  operation:
    kind: http_check
    url: https://app.example.test/health
    expected_status: 200
    timeout_seconds: 10
```

## exec：执行自己的脚本

准备流程目录内的 `scripts/verify.sh` 并随包登记。Shell 默认低权限，需要修改主机时声明 `execution: host`；保留超时、取消和回执。

```yaml
- id: verify
  phase: verify
  scope: per_node
  hosts: worker
  operation:
    kind: exec
    script: scripts/verify.sh
    timeout_seconds: 30
    environment:
      EXPECTED_VERSION: "1.2.3"
```

## reboot：重启并等待恢复

这是会重启主机的操作。仅在计划维护时使用，在流程中明确声明重启步骤。`recovery_timeout_seconds` 范围 1–86400；可选 `expected_kernel` 固定恢复后的内核 release。成功依据新 boot ID 和恢复证据，不能用一次 systemctl 返回值替代；重启后再安排健康步骤。

```yaml
- id: reboot
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: reboot
    recovery_timeout_seconds: 600
```

## 组合、预览和常见失败

顺序通常是 prepare 预检 → rollout 写配置/制品/服务 → verify 健康检查。把需要的步骤组合进一个流程，执行 `validate workflows`、`register workflows`、`run --dry-run`，确认实际节点和操作，再 `run --wait`。软件包、重启等操作应在可丢弃目标先验证。

| 现象 | 检查位置 |
| --- | --- |
| 文件路径受保护/目录不存在 | 父目录、受保护路径及当前任务授权 |
| Shell 无法写入但 file 可以 | Shell 默认低权限；需要 root 时声明 `execution: host` |
| 制品 source 未配置/摘要不符 | 每台 Agent 的 artifact_sources；真实文件 SHA-256 和长度 |
| HTTP origin 被拒绝 | 流程中的 URL、TLS 配置及实际端口 |
| service 成功但业务仍旧 | 实际运行摘要、配置加载机制及业务 API；active 不能证明版本 |
| 节点离线/等待重启恢复 | 原 Run、Agent 心跳、boot ID、内核及恢复超时 |
| 插件不可用或授权不匹配 | 已批准的主机操作；无需节点白名单 |

`plugin` 操作由 `compile plugins -o json` 生成，其 `call` 含固定插件身份、动作、执行位置和业务 spec。不要手填摘要。完整主机操作示例见[插件](../how-to/plugins.md)，控制器连接示例见[外部 Kubernetes](../how-to/external-kubernetes.md)。全部字段见[API 类型](api-types.md)。
