# Converyn HTTP API

[English](../../en/reference/api-reference.md) | [简体中文](../../zh-CN/reference/api-reference.md)

依据 [OpenAPI](../../schema/openapi.json) 生成。完整结构见[字段表](api-types.md)，操作模型见[资源说明](resources.md)，执行路径见[使用指南](../how-to/runs.md)，流程定义见[编写指南](../how-to/workflows.md)。

## 身份与执行

业务 API 使用 `/api/v1`。HTTPS 使用客户端证书，并要求 `Authorization: Bearer <个人令牌>` 与 `x-converyn-cluster: <控制面名称>`。新人员默认无业务权限；平台角色与流程授权决定操作能力及流程／预设范围。任务、日志及归档沿用原执行的授权范围。详见[人员与流程授权](../how-to/access.md)。无密码登录和人工审批。

OP 在环境文件或 Git 检出目录维护流程，通过 CLI 登记不可变快照。执行只接收流程修订、预设目标、开放参数及请求 ID。服务端编译 Plan，提交时检查实际摘要和目标快照。旧 catalog、审批、脚本上传和直接 Plan 上传入口已移除。

预览不是执行。提交成功返回 Run ID，使用原 Run 查询实际终态和回执。参数仅影响本次执行；显式启用持续核对时沿用该次快照。网络重试复用原请求 ID，不另建任务代替未知结果处置。

## 一致性、限制和日志

普通读取确认多数派，Follower 转发保留原始身份。业务变更与命令审计同事务提交；审计自动记录人员、证书指纹、流程修订和结果，不是全部 HTTP 访问日志。

请求体最多 4 MiB，流程包最多 1 MiB，单定义最多 64 KiB，最终开放参数快照最多 256 KiB。流程请求 ID 最多 124 字符。`*_ms` 为 Unix 毫秒，`*_seconds` 为秒。

错误包含 code、message、retryable、request_id。429 应退避；503 或断连可能是提交结果未知，应先查询原请求再重试。日志分块为 64 KiB；SSE 和下载持续复核身份。原始日志独立于审计和备份存储。

## 路由

### `GET /`

HTTPS 监听仍检查客户端证书。

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `text/html`.

流式/文本返回，按Content-Type读取。

### `GET /api/v1/access`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `revision` | integer | 必填 |
| `groups` | map<string, [Group](api-types.md#crates_core_identities_access_group)> | 必填 |
| `bindings` | map<string, [Binding](api-types.md#crates_core_identities_access_binding)> | 必填 |
| `roles` | [Role](api-types.md#crates_core_identities_access_role)[] | 必填 |
| `workflows` | object[] | 必填 |

### `POST /api/v1/access`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/access/check`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `action` | query | string | 可选 |
| `workflow` | query | string | 可选 |
| `preset` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `allowed` | boolean | 必填 |
| `action` | [Action](api-types.md#crates_core_identities_access_action) | 必填 |
| `workflow` | string 或 null | 必填 |
| `preset` | string 或 null | 必填 |
| `reason` | string | 必填 |

### `GET /api/v1/access/me`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `revision` | integer | 必填 |
| `bindings` | [Binding](api-types.md#crates_core_identities_access_binding)[] | 必填 |
| `platform` | boolean | 必填 |
| `access` | boolean | 必填 |
| `super_admin` | boolean | 必填 |

### `GET /api/v1/agents/{node}/attempts/{run}/{work}`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) 或 null | 必填 |
| `work` | [Work](api-types.md#crates_core_model_domain_work) | 必填 |
| `attempt` | [Attempt](api-types.md#crates_core_orchestration_machine_types_attempt) | 必填 |
| `run_status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `project` | string 或 null | 必填 |
| `owner_uid` | string | 必填 |

### `POST /api/v1/agents/{node}/commands`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `type=preparation`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_preparation_change) | 必填 |
| `type` | `"preparation"` | 必填 |

拒绝额外字段。

区分字段 `type=claim`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `input_sha256` | string 或 null | 可省略 |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `authorized_ms` | integer 或 null | 可省略 |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `grant` | string | 必填 |
| `boot_id` | string | 必填 |
| `session_id` | string | 必填 |
| `type` | `"claim"` | 必填 |

拒绝额外字段。

区分字段 `type=report`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `grant` | string | 必填 |
| `sequence` | integer | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `receipt` | [Receipt](api-types.md#crates_core_model_receipt_receipt) 或 null | 可省略 |
| `type` | `"report"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/agents/{node}/heartbeat`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `diagnostic` | string 或 null | 可省略 |
| `inventory` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) 或 null | 可省略 |
| `resources` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) 或 null | 可省略 |
| `inventory_checked_ms` | integer 或 null | 可省略 |
| `capabilities` | [Identity](api-types.md#crates_plugin_api_lib_identity)[] | 可省略 |
| `hardware_sha256` | string 或 null | 可省略 |
| `boot_id` | string | 必填 |
| `session_id` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/agents/{node}/log-storage`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `enabled` | boolean | 必填 |
| `available` | boolean | 必填 |
| `store_id` | string 或 null | 必填 |

### `POST /api/v1/agents/{node}/logs/{run}/{work}/{stream}`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `stream` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant` | string 或 null | 可省略 |
| `offset` | integer | 必填 |
| `data` | string | 必填 |
| `sha256` | string | 必填 |
| `complete` | boolean | 可省略 |
| `missing` | boolean | 可省略 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `next_offset` | integer | 必填 |

### `POST /api/v1/agents/{node}/logs/{run}/{work}/{stream}/live`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `stream` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant` | string 或 null | 可省略 |
| `offset` | integer | 必填 |
| `data` | string | 必填 |
| `sha256` | string | 必填 |
| `complete` | boolean | 可省略 |
| `missing` | boolean | 可省略 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `next_offset` | integer | 必填 |

### `POST /api/v1/agents/{node}/renew`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string | 必填 |
| `csr_pem` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) 或 null | 必填 |

### `POST /api/v1/agents/{node}/renew/activate`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |

### `GET /api/v1/agents/{node}/runs/{run}/work/{work}/artifacts/{sha}`

仅对应 Node 的当前身份及本节点进行中的 Run 分配；支持断点续传。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |
| `Range` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/octet-stream`.

流式/文本返回，按Content-Type读取。

成功响应 HTTP 206:

媒体类型 `application/octet-stream`.

流式/文本返回，按Content-Type读取。

### `GET /api/v1/agents/{node}/runs/{run}/work/{work}/inputs`

仅对应 Node 的当前身份与已提交 grant；管理员不能读取明文。响应禁止缓存。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `values` | map<string, string> | 必填 |
| `redactions` | integer[][] | 必填 |

### `GET /api/v1/agents/{node}/scripts/{sha}`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sha256` | string | 必填 |
| `content` | string | 必填 |

### `GET /api/v1/agents/{node}/work`

对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |
| `after` | query | integer | 可选 |
| `wait_ms` | query | integer | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `cursor` | integer 或 null | 必填 |
| `node_uid` | string | 必填 |
| `work` | object[] | 必填 |
| `artifact_pins` | map<string, integer> | 必填 |
| `maintenance_until_ms` | integer 或 null | 必填 |

### `GET /api/v1/artifacts`

平台运维权限；上传文件在所有当前投票成员核验落盘后登记。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `artifacts` | object[] | 必填 |

### `POST /api/v1/artifacts/{sha}/chunks`

平台运维权限；上传文件在所有当前投票成员核验落盘后登记。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `size` | query | integer | 必填 |
| `offset` | query | integer | 可选 |

请求体：

媒体类型 `application/octet-stream`.

二进制分块；按声明的长度和偏移上传。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `next_offset` | integer | 可省略 |

### `POST /api/v1/artifacts/{sha}/complete`

平台运维权限；上传文件在所有当前投票成员核验落盘后登记。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `size` | query | integer | 必填 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/audit`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | integer | 可选 |
| `limit` | query | integer | 可选 |
| `stream` | query | string | 可选 |
| `view` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `records` | 任意 JSON[] | 必填 |
| `stream` | string | 必填 |
| `view` | string | 必填 |
| `next_cursor` | integer 或 null | 必填 |

### `GET /api/v1/connections`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `connections` | object[] | 必填 |

### `GET /api/v1/controlplane/backup`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/vnd.converyn.backup.v2`.

流式/文本返回，按Content-Type读取。

### `POST /api/v1/controlplane/init`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

类型：map<string, object>.

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `initialized` | boolean | 必填 |

### `GET /api/v1/controlplane/members`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | 必填 |
| `config_fingerprint` | string | 必填 |
| `voters` | integer[] | 必填 |
| `operations` | [Catalog](api-types.md#crates_core_orchestration_controlplane_catalog) | 必填 |
| `replication` | map<string, [RaftLogId](api-types.md#raftlogid) 或 null> 或 null | 必填 |

### `POST /api/v1/controlplane/members`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `old_id` | integer | 必填 |
| `new_id` | integer | 必填 |
| `address` | string | 必填 |
| `old_fingerprint` | string | 必填 |
| `new_fingerprint` | string | 必填 |
| `expected_membership` | string | 必填 |
| `voters` | integer[] | 必填 |
| `evidence` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/controlplane/metrics`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `consistency` | `"local"` | 必填 |
| `metrics` | [RaftMetrics](api-types.md#raftmetrics) | 必填 |
| `writes` | object | 必填 |

### `GET /api/v1/controlplane/prometheus`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `text/plain`.

流式/文本返回，按Content-Type读取。

### `POST /api/v1/controlplane/snapshot`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 202:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `requested` | boolean | 必填 |

### `GET /api/v1/credentials`

具名管理员；引导凭据仅能创建首个管理员。返回内容不包含私钥或令牌。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `credentials` | object[] | 必填 |

### `POST /api/v1/credentials`

具名管理员；引导凭据仅能创建首个管理员。返回内容不包含私钥或令牌。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `action=add_identity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) | 必填 |
| `action` | `"add_identity"` | 必填 |

拒绝额外字段。

区分字段 `action=create`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `super_admin` | boolean | 可省略 |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) | 必填 |
| `credential` | [Credential](api-types.md#crates_core_identities_personnel_credential) | 必填 |
| `request_id` | string | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"create"` | 必填 |

拒绝额外字段。

区分字段 `action=update`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `disabled` | boolean | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"update"` | 必填 |

拒绝额外字段。

区分字段 `action=issue`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `credential` | [Credential](api-types.md#crates_core_identities_personnel_credential) | 必填 |
| `expected_revision` | integer | 必填 |
| `rotate` | boolean | 必填 |
| `overlap_seconds` | integer | 必填 |
| `request_id` | string | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"issue"` | 必填 |

拒绝额外字段。

区分字段 `action=revoke`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"revoke"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/credentials/{id}`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `credential` | object | 必填 |
| `storage` | map<string, string> | 必填 |

### `GET /api/v1/deployments/{id}`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `generation` | integer | 必填 |
| `applied_generation` | integer 或 null | 可省略 |
| `last_started_generation` | integer | 必填 |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | 必填 |
| `active_run` | string 或 null | 可省略 |
| `last_run` | string 或 null | 可省略 |
| `condition` | string | 必填 |

### `GET /api/v1/deployments/{id}/drift`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `policy` | [Policy](api-types.md#crates_core_orchestration_drift_policy) 或 null | 必填 |

### `GET /api/v1/executions`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `executions` | map<string, [Execution](api-types.md#crates_core_orchestration_workflow_execution)> | 必填 |

### `GET /api/v1/history`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `records` | map<string, [Record](api-types.md#crates_core_orchestration_history_record)> | 必填 |
| `keep_runs` | integer | 必填 |
| `definition_hot_bytes` | integer | 必填 |
| `configuration_revision` | integer | 必填 |
| `audit` | 任意 JSON[] | 必填 |
| `next_cursor` | string 或 null | 必填 |
| `run_sequence` | integer | 必填 |

### `POST /api/v1/history`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `action=clear_runs`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `expected_sequence` | integer | 必填 |
| `backup_sha256` | string | 必填 |
| `evidence` | string | 必填 |
| `action` | `"clear_runs"` | 必填 |

拒绝额外字段。

区分字段 `action=archive`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kind` | [Kind](api-types.md#crates_core_orchestration_history_kind) | 必填 |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"archive"` | 必填 |

拒绝额外字段。

区分字段 `action=configure`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `expected_revision` | integer | 必填 |
| `keep_runs` | integer | 必填 |
| `definition_hot_bytes` | integer | 必填 |
| `evidence` | string | 必填 |
| `action` | `"configure"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/identities`

具名管理员；引导凭据仅能创建首个管理员。返回内容不包含私钥或令牌。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `identities` | map<string, [Identity](api-types.md#crates_core_identities_personnel_identity)> | 必填 |

### `POST /api/v1/identities`

具名管理员；引导凭据仅能创建首个管理员。返回内容不包含私钥或令牌。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `action=add_identity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) | 必填 |
| `action` | `"add_identity"` | 必填 |

拒绝额外字段。

区分字段 `action=create`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `super_admin` | boolean | 可省略 |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) | 必填 |
| `credential` | [Credential](api-types.md#crates_core_identities_personnel_credential) | 必填 |
| `request_id` | string | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"create"` | 必填 |

拒绝额外字段。

区分字段 `action=update`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `disabled` | boolean | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"update"` | 必填 |

拒绝额外字段。

区分字段 `action=issue`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `credential` | [Credential](api-types.md#crates_core_identities_personnel_credential) | 必填 |
| `expected_revision` | integer | 必填 |
| `rotate` | boolean | 必填 |
| `overlap_seconds` | integer | 必填 |
| `request_id` | string | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"issue"` | 必填 |

拒绝额外字段。

区分字段 `action=revoke`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"revoke"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/identity/bootstrap`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `id` | string | 必填 |
| `node_uid` | string | 必填 |
| `token_sha256` | string | 必填 |
| `csr_pem` | string | 必填 |
| `ttl_seconds` | integer | 必填 |
| `recover` | boolean | 可省略 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant` | [Bootstrap](api-types.md#crates_core_identities_registration_bootstrap) | 必填 |

### `POST /api/v1/identity/enroll`

一次性节点引导凭据。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string | 必填 |
| `grant_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `hardware_sha256` | string | 必填 |
| `token_sha256` | string | 必填 |
| `csr_pem` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) 或 null | 必填 |

### `GET /api/v1/identity/me`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `capabilities` | object | 必填 |
| `principal` | string 或 null | 必填 |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) 或 null | 必填 |
| `credential` | object 或 null | 必填 |

### `GET /api/v1/identity/nodes`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `nodes` | object[] | 必填 |
| `next_cursor` | string 或 null | 必填 |

### `POST /api/v1/identity/retire`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `expected_fingerprint` | string | 必填 |
| `evidence` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |

### `GET /api/v1/node-groups`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_groups` | map<string, [NodeGroup](api-types.md#crates_core_model_domain_nodegroup)> | 必填 |
| `administration` | [Administration](api-types.md#crates_core_orchestration_administration_administration) | 必填 |
| `resource_owners` | map<string, string> | 必填 |
| `next_cursor` | string 或 null | 必填 |

### `POST /api/v1/node-groups/commands`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `action=fence_floor`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `expected_sequence` | integer | 必填 |
| `floor` | integer | 必填 |
| `evidence` | string | 必填 |
| `action` | `"fence_floor"` | 必填 |

拒绝额外字段。

区分字段 `action=target`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `target` | [ExternalTarget](api-types.md#crates_core_model_target_externaltarget) | 必填 |
| `action` | `"target"` | 必填 |

拒绝额外字段。

区分字段 `action=delete_node_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `reason` | string | 必填 |
| `action` | `"delete_node_group"` | 必填 |

拒绝额外字段。

区分字段 `action=maintenance_window`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `window` | [MaintenanceWindow](api-types.md#crates_core_orchestration_administration_maintenancewindow) | 必填 |
| `expected_revision` | integer | 必填 |
| `evidence` | string | 必填 |
| `action` | `"maintenance_window"` | 必填 |

拒绝额外字段。

区分字段 `action=create_node_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `group` | [NodeGroup](api-types.md#crates_core_model_domain_nodegroup) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"create_node_group"` | 必填 |

拒绝额外字段。

区分字段 `action=update_node_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `group` | [NodeGroup](api-types.md#crates_core_model_domain_nodegroup) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"update_node_group"` | 必填 |

拒绝额外字段。

区分字段 `action=maintenance`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `enabled` | boolean | 必填 |
| `action` | `"maintenance"` | 必填 |

拒绝额外字段。

区分字段 `action=ownership`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `resource` | string | 必填 |
| `expected_owner` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `new_owner` | string 或 null | 可省略 |
| `evidence` | string | 必填 |
| `action` | `"ownership"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/nodes`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `nodes` | map<string, [NodeObservation](api-types.md#crates_core_orchestration_machine_types_nodeobservation)> | 必填 |
| `next_cursor` | string 或 null | 必填 |

### `GET /api/v1/nodes/{id}`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `nodes` | map<string, [NodeObservation](api-types.md#crates_core_orchestration_machine_types_nodeobservation)> | 必填 |

### `GET /api/v1/plugins`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `plugins` | [Descriptor](api-types.md#crates_plugin_api_lib_descriptor)[] | 必填 |
| `manifest_sha256` | string | 必填 |

### `GET /api/v1/prototype/agents/{node}/work`

仅回环开发测试接口。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `node` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-hardware` | header | string | 可选 |
| `x-converyn-session` | header | string | 可选 |
| `after` | query | integer | 可选 |
| `wait_ms` | query | integer | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `cursor` | integer 或 null | 必填 |
| `node_uid` | string | 必填 |
| `work` | object[] | 必填 |
| `artifact_pins` | map<string, integer> | 必填 |
| `maintenance_until_ms` | integer 或 null | 必填 |

### `POST /api/v1/prototype/commands`

仅回环开发测试接口。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `type=access`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_access_change) | 必填 |
| `type` | `"access"` | 必填 |

拒绝额外字段。

区分字段 `type=retry_inputs`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `type` | `"retry_inputs"` | 必填 |

拒绝额外字段。

区分字段 `type=materialize`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `now_ms` | integer | 必填 |
| `type` | `"materialize"` | 必填 |

拒绝额外字段。

区分字段 `type=workflow`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_workflow_change) | 必填 |
| `type` | `"workflow"` | 必填 |

拒绝额外字段。

区分字段 `type=personnel`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_personnel_change) | 必填 |
| `type` | `"personnel"` | 必填 |

拒绝额外字段。

区分字段 `type=prepared`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `command` | [Command](api-types.md#crates_core_orchestration_machine_types_command) | 必填 |
| `contracts` | 任意 JSON | 必填 |
| `type` | `"prepared"` | 必填 |

拒绝额外字段。

区分字段 `type=history`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_history_change) | 必填 |
| `type` | `"history"` | 必填 |

拒绝额外字段。

区分字段 `type=controlplane`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_controlplane_change) | 必填 |
| `type` | `"controlplane"` | 必填 |

拒绝额外字段。

区分字段 `type=preparation`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_preparation_change) | 必填 |
| `type` | `"preparation"` | 必填 |

拒绝额外字段。

区分字段 `type=pause_run`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `paused` | boolean | 必填 |
| `expected_revision` | integer | 必填 |
| `type` | `"pause_run"` | 必填 |

拒绝额外字段。

区分字段 `type=authorized`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `authorization` | [Authorization](api-types.md#crates_core_identities_registration_authorization) | 必填 |
| `command` | [Command](api-types.md#crates_core_orchestration_machine_types_command) | 必填 |
| `type` | `"authorized"` | 必填 |

拒绝额外字段。

区分字段 `type=identity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_registration_change) | 必填 |
| `type` | `"identity"` | 必填 |

拒绝额外字段。

区分字段 `type=configure_drift`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `interval_seconds` | integer 或 null | 可省略 |
| `type` | `"configure_drift"` | 必填 |

拒绝额外字段。

区分字段 `type=reconcile_drift`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `now_ms` | integer | 必填 |
| `expected_generation` | integer | 必填 |
| `type` | `"reconcile_drift"` | 必填 |

拒绝额外字段。

区分字段 `type=administer`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_administration_change) | 必填 |
| `type` | `"administer"` | 必填 |

拒绝额外字段。

区分字段 `type=set_certificate_binding`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_identity_change) | 必填 |
| `type` | `"set_certificate_binding"` | 必填 |

拒绝额外字段。

区分字段 `type=apply_deployment`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | 必填 |
| `type` | `"apply_deployment"` | 必填 |

拒绝额外字段。

区分字段 `type=reconcile_deployment`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `type` | `"reconcile_deployment"` | 必填 |

拒绝额外字段。

区分字段 `type=observe_node`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `diagnostic` | string 或 null | 可省略 |
| `inventory` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) 或 null | 可省略 |
| `resources` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) 或 null | 可省略 |
| `inventory_checked_ms` | integer 或 null | 可省略 |
| `capabilities` | [Identity](api-types.md#crates_plugin_api_lib_identity)[] | 可省略 |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `hardware_sha256` | string 或 null | 可省略 |
| `node_uid` | string | 必填 |
| `boot_id` | string | 必填 |
| `session_id` | string | 必填 |
| `observed_ms` | integer | 必填 |
| `type` | `"observe_node"` | 必填 |

拒绝额外字段。

区分字段 `type=put_script`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sha256` | string | 必填 |
| `content` | string | 必填 |
| `type` | `"put_script"` | 必填 |

拒绝额外字段。

区分字段 `type=create_run`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) 或 null | 可省略 |
| `run_id` | string | 必填 |
| `idempotency_key` | string | 必填 |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | 必填 |
| `type` | `"create_run"` | 必填 |

拒绝额外字段。

区分字段 `type=claim`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `input_sha256` | string 或 null | 可省略 |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `authorized_ms` | integer 或 null | 可省略 |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `grant` | string | 必填 |
| `boot_id` | string | 必填 |
| `session_id` | string | 必填 |
| `type` | `"claim"` | 必填 |

拒绝额外字段。

区分字段 `type=report`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `grant` | string | 必填 |
| `sequence` | integer | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `receipt` | [Receipt](api-types.md#crates_core_model_receipt_receipt) 或 null | 可省略 |
| `type` | `"report"` | 必填 |

拒绝额外字段。

区分字段 `type=cancel`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `type` | `"cancel"` | 必填 |

拒绝额外字段。

区分字段 `type=mark_uncertain`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `reason` | string 或 null | 可省略 |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `type` | `"mark_uncertain"` | 必填 |

拒绝额外字段。

区分字段 `type=resolve`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `actor` | string | 必填 |
| `evidence` | string | 必填 |
| `type` | `"resolve"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/runs`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |
| `workflow` | query | string | 可选 |
| `preset` | query | string | 可选 |
| `actor` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `runs` | object[] | 必填 |
| `next_cursor` | string 或 null | 必填 |

### `GET /api/v1/runs/{id}`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

类型：任意 JSON.

### `POST /api/v1/runs/{id}/cancel`

任务提交者或管理员，服务端和状态机检查实际身份。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/runs/{id}/pause`

任务提交者或管理员，服务端和状态机检查实际身份。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `paused` | boolean | 必填 |
| `expected_revision` | integer | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/runs/{id}/resolve`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `actor` | string | 必填 |
| `evidence` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/runs/{id}/uncertain`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `reason` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/runs/{id}/work/{work}/definition`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `work` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sha256` | string | 必填 |
| `content` | string | 必填 |
| `rendered_content` | string | 必填 |

### `GET /api/v1/runs/{run}/logs/{work}/{stream}`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `stream` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `offset` | query | integer | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `offset` | integer | 必填 |
| `next_offset` | integer | 必填 |
| `data` | string | 必填 |
| `complete` | boolean | 必填 |
| `expired` | boolean | 必填 |
| `truncated` | boolean | 必填 |
| `source` | string | 可省略 |
| `sha256` | string 或 null | 可省略 |
| `verified` | boolean | 可省略 |
| `missing_ranges` | [integer, integer][] | 可省略 |
| `condition` | string | 可省略 |

### `GET /api/v1/runs/{run}/logs/{work}/{stream}/download`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `stream` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/octet-stream`.

流式/文本返回，按Content-Type读取。

### `GET /api/v1/runs/{run}/logs/{work}/{stream}/events`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `run` | path | string | 必填 |
| `work` | path | string | 必填 |
| `stream` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `offset` | query | integer | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `text/event-stream`.

流式/文本返回，按Content-Type读取。

### `GET /api/v1/secrets`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `secrets` | object[] | 必填 |

### `POST /api/v1/secrets`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `name` | string | 必填 |
| `version` | string | 必填 |
| `value` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/security/certificates`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `principal` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `fingerprints` | string[] | 必填 |
| `evidence` | string | 必填 |

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/security/certificates`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `identities` | [Identities](api-types.md#crates_core_identities_identity_identities) | 必填 |

### `GET /api/v1/targets`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `targets` | map<string, [ExternalTarget](api-types.md#crates_core_model_target_externaltarget)> | 必填 |
| `next_cursor` | string 或 null | 必填 |

### `POST /api/v1/targets/commands`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

区分字段 `action=fence_floor`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `expected_sequence` | integer | 必填 |
| `floor` | integer | 必填 |
| `evidence` | string | 必填 |
| `action` | `"fence_floor"` | 必填 |

拒绝额外字段。

区分字段 `action=target`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `target` | [ExternalTarget](api-types.md#crates_core_model_target_externaltarget) | 必填 |
| `action` | `"target"` | 必填 |

拒绝额外字段。

区分字段 `action=delete_node_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `reason` | string | 必填 |
| `action` | `"delete_node_group"` | 必填 |

拒绝额外字段。

区分字段 `action=maintenance_window`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `window` | [MaintenanceWindow](api-types.md#crates_core_orchestration_administration_maintenancewindow) | 必填 |
| `expected_revision` | integer | 必填 |
| `evidence` | string | 必填 |
| `action` | `"maintenance_window"` | 必填 |

拒绝额外字段。

区分字段 `action=create_node_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `group` | [NodeGroup](api-types.md#crates_core_model_domain_nodegroup) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"create_node_group"` | 必填 |

拒绝额外字段。

区分字段 `action=update_node_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `group` | [NodeGroup](api-types.md#crates_core_model_domain_nodegroup) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"update_node_group"` | 必填 |

拒绝额外字段。

区分字段 `action=maintenance`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `enabled` | boolean | 必填 |
| `action` | `"maintenance"` | 必填 |

拒绝额外字段。

区分字段 `action=ownership`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `resource` | string | 必填 |
| `expected_owner` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `new_owner` | string 或 null | 可省略 |
| `evidence` | string | 必填 |
| `action` | `"ownership"` | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/workflows`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `after` | query | string | 可选 |
| `limit` | query | integer | 可选 |
| `status` | query | string | 可选 |
| `search` | query | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflows` | object[] | 必填 |
| `next_cursor` | string 或 null | 必填 |

### `POST /api/v1/workflows/register`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflow` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | 必填 |
| `expected_revision` | integer | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /api/v1/workflows/{id}`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `actions` | object | 必填 |
| `workflow` | [Workflow](api-types.md#crates_core_orchestration_workflow_workflow) | 必填 |
| `definition` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | 必填 |
| `reconciliation` | object[] | 必填 |

### `POST /api/v1/workflows/{id}/enabled`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `enabled` | boolean | 必填 |
| `expected_revision` | integer | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `POST /api/v1/workflows/{id}/preview`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflow_revision` | string | 必填 |
| `target` | string | 必填 |
| `parameters` | map<string, 任意 JSON> | 可省略 |
| `request_id` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request` | [Request](api-types.md#crates_core_orchestration_workflow_request) | 必填 |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | 必填 |
| `parameters` | map<string, 任意 JSON> | 必填 |
| `rendered_scripts` | map<string, string> | 必填 |

### `POST /api/v1/workflows/{id}/runs`

已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `id` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflow_uid` | string | 必填 |
| `workflow_revision` | string | 必填 |
| `target` | string | 必填 |
| `parameters` | map<string, 任意 JSON> | 可省略 |
| `request_id` | string | 必填 |
| `plan_sha256` | string | 必填 |
| `target_sha256` | string | 必填 |

拒绝额外字段。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

### `GET /assets/app.js`

HTTPS 监听仍检查客户端证书。

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `text/javascript`.

流式/文本返回，按Content-Type读取。

### `GET /assets/index.css`

HTTPS 监听仍检查客户端证书。

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `text/css`.

流式/文本返回，按Content-Type读取。

### `GET /assets/version.json`

HTTPS 监听仍检查客户端证书。

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `version` | string | 必填 |

### `GET /health/live`

HTTPS 监听仍检查客户端证书。

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `live` | boolean | 必填 |
| `stage` | string | 必填 |
| `protocol` | integer | 必填 |
| `plugin_manifest` | string | 必填 |
| `node_id` | integer | 必填 |
| `control_plane` | string | 必填 |
| `pristine` | boolean | 必填 |
| `log_storage` | object 或 null | 必填 |

### `GET /health/ready`

管理员。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ready` | boolean | 必填 |

### `POST /internal/raft/append`

仅控制器 peer 身份，协议和插件清单必须匹配。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-plugin-manifest` | header | string | 必填 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `prev_log_id` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `entries` | [RaftEntry](api-types.md#raftentry)[] | 必填 |
| `leader_commit` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |

成功响应 HTTP 200:

媒体类型 `application/json`.

object

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `Ok` | string 或 object 或 object | 必填 |

object

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `Err` | 任意 JSON | 必填 |

### `GET /internal/raft/artifacts/{sha}`

仅控制器 peer 身份，协议和插件清单必须匹配。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-plugin-manifest` | header | string | 必填 |
| `size` | query | integer | 必填 |
| `Range` | header | string | 可选 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/octet-stream`.

流式/文本返回，按Content-Type读取。

成功响应 HTTP 206:

媒体类型 `application/octet-stream`.

流式/文本返回，按Content-Type读取。

### `POST /internal/raft/artifacts/{sha}/chunks`

仅控制器 peer 身份，协议和插件清单必须匹配。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-plugin-manifest` | header | string | 必填 |
| `size` | query | integer | 必填 |
| `offset` | query | integer | 可选 |

请求体：

媒体类型 `application/octet-stream`.

二进制分块；按声明的长度和偏移上传。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |

### `POST /internal/raft/artifacts/{sha}/complete`

仅控制器 peer 身份，协议和插件清单必须匹配。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `sha` | path | string | 必填 |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-plugin-manifest` | header | string | 必填 |
| `size` | query | integer | 必填 |

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |

### `POST /internal/raft/snapshot`

仅控制器 peer 身份，协议和插件清单必须匹配。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-plugin-manifest` | header | string | 必填 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `meta` | [RaftSnapshotMeta](api-types.md#raftsnapshotmeta) | 必填 |
| `offset` | integer | 必填 |
| `data` | integer[] | 必填 |
| `done` | boolean | 必填 |

成功响应 HTTP 200:

媒体类型 `application/json`.

object

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `Ok` | object | 必填 |

object

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `Err` | 任意 JSON | 必填 |

### `POST /internal/raft/vote`

仅控制器 peer 身份，协议和插件清单必须匹配。

内部协议或受限本地测试接口；访问边界见本接口权限。

| 参数 | 位置 | 类型 | 要求 |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | 必填 |
| `x-request-id` | header | string | 可选 |
| `x-converyn-protocol` | header | `27` | 必填 |
| `x-converyn-plugin-manifest` | header | string | 必填 |

请求体：

媒体类型 `application/json`.

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `last_log_id` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |

成功响应 HTTP 200:

媒体类型 `application/json`.

object

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `Ok` | object | 必填 |

object

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `Err` | 任意 JSON | 必填 |

### `GET /openapi.json`

HTTPS 监听仍检查客户端证书。

无JSON请求体。

成功响应 HTTP 200:

媒体类型 `application/json`.

类型：object.

## 维护与验证

`python3 tools/generate_api.py`

`python3 tools/render_api_docs.py --check`
