# Converyn API 完整字段表

[English](../../en/reference/api-types.md) | [简体中文](../../zh-CN/reference/api-types.md)

由 OpenAPI 契约生成。各类型以源模块和名称标识，列出字段、JSON 类型、必填项和默认值。路由权限见 [API 参考](api-reference.md)。

<a id="apierror"></a>

### ApiError

`ApiError`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `code` | string | 必填 |
| `message` | string | 必填 |
| `retryable` | boolean | 必填 |
| `request_id` | string | 必填 |

<a id="logstreamerror"></a>

### LogStreamError

`LogStreamError`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `status` | integer | 必填 |
| `code` | string | 必填 |
| `message` | string | 必填 |
| `retryable` | boolean | 必填 |
| `request_id` | string | 必填 |

<a id="page"></a>

### Page

`Page`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `items` | object[] | 必填 |
| `next_cursor` | string 或 null | 必填 |

<a id="raftappendentriesrequest"></a>

### RaftAppendEntriesRequest

`RaftAppendEntriesRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `prev_log_id` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `entries` | [RaftEntry](api-types.md#raftentry)[] | 必填 |
| `leader_commit` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |

<a id="raftentry"></a>

### RaftEntry

`RaftEntry`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `log_id` | [RaftLogId](api-types.md#raftlogid) | 必填 |
| `payload` | `"Blank"` 或 object 或 object | 必填 |

<a id="raftinstallsnapshotrequest"></a>

### RaftInstallSnapshotRequest

`RaftInstallSnapshotRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `meta` | [RaftSnapshotMeta](api-types.md#raftsnapshotmeta) | 必填 |
| `offset` | integer | 必填 |
| `data` | integer[] | 必填 |
| `done` | boolean | 必填 |

<a id="raftleaderid"></a>

### RaftLeaderId

`RaftLeaderId`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `term` | integer | 必填 |
| `node_id` | integer | 必填 |

<a id="raftlogid"></a>

### RaftLogId

`RaftLogId`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `leader_id` | [RaftLeaderId](api-types.md#raftleaderid) | 必填 |
| `index` | integer | 必填 |

<a id="raftmembership"></a>

### RaftMembership

`RaftMembership`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `configs` | integer[][] | 必填 |
| `nodes` | map<string, object> | 必填 |

<a id="raftmetrics"></a>

### RaftMetrics

`RaftMetrics`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `running_state` | object 或 object | 必填 |
| `id` | integer | 必填 |
| `current_term` | integer | 必填 |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `last_log_index` | integer 或 null | 必填 |
| `last_applied` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `snapshot` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `purged` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `state` | string | 必填 |
| `current_leader` | integer 或 null | 必填 |
| `millis_since_quorum_ack` | integer 或 null | 必填 |
| `membership_config` | [RaftStoredMembership](api-types.md#raftstoredmembership) | 必填 |
| `replication` | map<string, [RaftLogId](api-types.md#raftlogid) 或 null> 或 null | 必填 |

<a id="raftsnapshotmeta"></a>

### RaftSnapshotMeta

`RaftSnapshotMeta`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `last_log_id` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `last_membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | 必填 |
| `snapshot_id` | string | 必填 |

<a id="raftstoredmembership"></a>

### RaftStoredMembership

`RaftStoredMembership`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `log_id` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |
| `membership` | [RaftMembership](api-types.md#raftmembership) | 必填 |

<a id="raftvote"></a>

### RaftVote

`RaftVote`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `leader_id` | [RaftLeaderId](api-types.md#raftleaderid) | 必填 |
| `committed` | boolean | 必填 |

<a id="raftvoterequest"></a>

### RaftVoteRequest

`RaftVoteRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | 必填 |
| `last_log_id` | [RaftLogId](api-types.md#raftlogid) 或 null | 必填 |

<a id="crates_core_identities_access_access"></a>

### Access

`crates_core_identities_access_Access`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `revision` | integer | 必填 |
| `groups` | map<string, [Group](api-types.md#crates_core_identities_access_group)> | 必填 |
| `bindings` | map<string, [Binding](api-types.md#crates_core_identities_access_binding)> | 必填 |

<a id="crates_core_identities_access_action"></a>

### Action

`crates_core_identities_access_Action`

`"view"`

`"preview"`

`"execute"`

`"maintain"`

`"control"`

`"platform"`

`"access"`

<a id="crates_core_identities_access_binding"></a>

### Binding

`crates_core_identities_access_Binding`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `subject` | [Subject](api-types.md#crates_core_identities_access_subject) | 必填 |
| `role` | [Role](api-types.md#crates_core_identities_access_role) | 必填 |
| `workflow` | string 或 null | 可省略 |
| `presets` | string[] | 可省略 |

拒绝额外字段。

<a id="crates_core_identities_access_change"></a>

### Change

`crates_core_identities_access_Change`

区分字段 `action=put_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `group` | [Group](api-types.md#crates_core_identities_access_group) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"put_group"` | 必填 |

拒绝额外字段。

区分字段 `action=delete_group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"delete_group"` | 必填 |

拒绝额外字段。

区分字段 `action=put_binding`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `binding` | [Binding](api-types.md#crates_core_identities_access_binding) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"put_binding"` | 必填 |

拒绝额外字段。

区分字段 `action=delete_binding`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"delete_binding"` | 必填 |

拒绝额外字段。

<a id="crates_core_identities_access_group"></a>

### Group

`crates_core_identities_access_Group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `members` | string[] | 必填 |

拒绝额外字段。

<a id="crates_core_identities_access_role"></a>

### Role

`crates_core_identities_access_Role`

`"viewer"`

`"executor"`

`"maintainer"`

`"platform_operator"`

`"access_admin"`

`"super_admin"`

<a id="crates_core_identities_access_subject"></a>

### Subject

`crates_core_identities_access_Subject`

区分字段 `kind=person`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kind` | `"person"` | 必填 |
| `uid` | string | 必填 |

拒绝额外字段。

区分字段 `kind=group`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kind` | `"group"` | 必填 |
| `uid` | string | 必填 |

拒绝额外字段。

<a id="crates_core_identities_certificate_signed"></a>

### Signed

`crates_core_identities_certificate_Signed`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `certificate_pem` | string | 必填 |
| `fingerprint` | string | 必填 |
| `expires_ms` | integer | 必填 |

<a id="crates_core_identities_identity_binding"></a>

### Binding

`crates_core_identities_identity_Binding`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `revision` | integer | 必填 |
| `fingerprints` | string[] | 必填 |

<a id="crates_core_identities_identity_change"></a>

### Change

`crates_core_identities_identity_Change`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `principal` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `fingerprints` | string[] | 必填 |
| `evidence` | string | 必填 |

<a id="crates_core_identities_identity_identities"></a>

### Identities

`crates_core_identities_identity_Identities`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `bindings` | map<string, [Binding](api-types.md#crates_core_identities_identity_binding)> | 必填 |
| `audit` | [Change](api-types.md#crates_core_identities_identity_change)[] | 必填 |

<a id="crates_core_identities_personnel_change"></a>

### Change

`crates_core_identities_personnel_Change`

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

<a id="crates_core_identities_personnel_credential"></a>

### Credential

`crates_core_identities_personnel_Credential`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `identity_uid` | string | 必填 |
| `certificate_pem` | string | 必填 |
| `fingerprint` | string | 必填 |
| `token_sha256` | string | 必填 |
| `not_before_ms` | integer | 必填 |
| `expires_ms` | integer | 必填 |
| `revoked_ms` | integer 或 null | 可省略 |
| `overlap_until_ms` | integer 或 null | 可省略 |

拒绝额外字段。

<a id="crates_core_identities_personnel_identity"></a>

### Identity

`crates_core_identities_personnel_Identity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `disabled` | boolean | 必填 |
| `revision` | integer | 必填 |

拒绝额外字段。

<a id="crates_core_identities_personnel_registry"></a>

### Registry

`crates_core_identities_personnel_Registry`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `access` | [Access](api-types.md#crates_core_identities_access_access) | 可省略 |
| `identities` | map<string, [Identity](api-types.md#crates_core_identities_personnel_identity)> | 必填 |
| `credentials` | map<string, [Credential](api-types.md#crates_core_identities_personnel_credential)> | 必填 |
| `operations` | map<string, string> | 必填 |

<a id="crates_core_identities_registration_authorization"></a>

### Authorization

`crates_core_identities_registration_Authorization`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string 或 null | 可省略 |
| `observed_ms` | integer 或 null | 可省略 |
| `token_sha256` | string 或 null | 可省略 |
| `principal` | string | 必填 |
| `fingerprint` | string | 必填 |

<a id="crates_core_identities_registration_bootstrap"></a>

### Bootstrap

`crates_core_identities_registration_Bootstrap`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `id` | string | 必填 |
| `node_uid` | string | 必填 |
| `token_sha256` | string | 必填 |
| `csr_sha256` | string | 必填 |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) | 必填 |
| `expires_ms` | integer | 必填 |
| `recover` | boolean | 必填 |
| `consumed` | string 或 null | 可省略 |
| `enrolled_certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) 或 null | 可省略 |

<a id="crates_core_identities_registration_change"></a>

### Change

`crates_core_identities_registration_Change`

区分字段 `action=bootstrap`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant` | [Bootstrap](api-types.md#crates_core_identities_registration_bootstrap) | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"bootstrap"` | 必填 |

拒绝额外字段。

区分字段 `action=enroll`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant_id` | string | 必填 |
| `bootstrap_sha256` | string | 必填 |
| `bootstrap_fingerprint` | string | 必填 |
| `node` | [Node](api-types.md#crates_core_identities_registration_node) | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"enroll"` | 必填 |

拒绝额外字段。

区分字段 `action=renew`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `pending` | [PendingRenewal](api-types.md#crates_core_identities_registration_pendingrenewal) | 必填 |
| `current_fingerprint` | string | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"renew"` | 必填 |

拒绝额外字段。

区分字段 `action=activate`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `request_id` | string | 必填 |
| `fingerprint` | string | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"activate"` | 必填 |

拒绝额外字段。

区分字段 `action=retire`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `expected_fingerprint` | string | 必填 |
| `evidence` | string | 必填 |
| `action` | `"retire"` | 必填 |

拒绝额外字段。

区分字段 `action=expire_bootstrap`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `now_ms` | integer | 必填 |
| `action` | `"expire_bootstrap"` | 必填 |

拒绝额外字段。

<a id="crates_core_identities_registration_node"></a>

### Node

`crates_core_identities_registration_Node`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `hardware_sha256` | string | 必填 |
| `token_sha256` | string | 必填 |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) | 必填 |
| `enrollment_id` | string | 必填 |
| `enrollment_digest` | string | 必填 |
| `pending` | [PendingRenewal](api-types.md#crates_core_identities_registration_pendingrenewal) 或 null | 可省略 |
| `retired` | boolean | 必填 |

<a id="crates_core_identities_registration_pendingrenewal"></a>

### PendingRenewal

`crates_core_identities_registration_PendingRenewal`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `id` | string | 必填 |
| `csr_sha256` | string | 必填 |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) | 必填 |

<a id="crates_core_identities_registration_proof"></a>

### Proof

`crates_core_identities_registration_Proof`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `principal` | string | 必填 |
| `fingerprint` | string | 必填 |
| `token_sha256` | string | 必填 |
| `hardware_sha256` | string 或 null | 可省略 |
| `session` | string 或 null | 可省略 |

<a id="crates_core_identities_registration_registry"></a>

### Registry

`crates_core_identities_registration_Registry`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `bootstraps` | map<string, [Bootstrap](api-types.md#crates_core_identities_registration_bootstrap)> | 必填 |
| `nodes` | map<string, [Node](api-types.md#crates_core_identities_registration_node)> | 必填 |
| `token_index` | map<string, string> | 必填 |
| `audit` | 任意 JSON[] | 必填 |

<a id="crates_core_model_artifact_artifactref"></a>

### ArtifactRef

`crates_core_model_artifact_ArtifactRef`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sha256` | string | 必填 |
| `size` | integer | 必填 |
| `locations` | [Location](api-types.md#crates_core_model_artifact_location)[] | 可省略 |

拒绝额外字段。

<a id="crates_core_model_artifact_location"></a>

### Location

`crates_core_model_artifact_Location`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `source` | string | 必填 |
| `path` | string | 必填 |

拒绝额外字段。

<a id="crates_core_model_contracts_outcome"></a>

### Outcome

`crates_core_model_contracts_Outcome`

区分字段 `result=value`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `result` | `"value"` | 必填 |
| `value` | 任意 JSON | 必填 |

拒绝额外字段。

区分字段 `result=error`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `result` | `"error"` | 必填 |
| `value` | string | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_hostssnapshot"></a>

### HostsSnapshot

`crates_core_model_domain_HostsSnapshot`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_group_uid` | string | 必填 |
| `revision` | integer | 必填 |
| `node_uids` | string[] | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_nodegroup"></a>

### NodeGroup

`crates_core_model_domain_NodeGroup`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `revision` | integer | 必填 |
| `node_uids` | string[] | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_operation"></a>

### Operation

`crates_core_model_domain_Operation`

区分字段 `kind=plugin`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `call` | [Call](api-types.md#crates_plugin_api_lib_call) | 必填 |
| `kind` | `"plugin"` | 必填 |

拒绝额外字段。

区分字段 `kind=http_check`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `url` | string | 必填 |
| `expected_status` | integer | 必填 |
| `body_sha256` | string 或 null | 可省略 |
| `timeout_seconds` | integer | 必填 |
| `kind` | `"http_check"` | 必填 |

拒绝额外字段。

区分字段 `kind=package`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `install_only` | boolean | 可省略 |
| `manager` | [Manager](api-types.md#crates_core_model_package_manager) | 必填 |
| `name` | string | 必填 |
| `version` | string | 必填 |
| `artifact` | [ArtifactRef](api-types.md#crates_core_model_artifact_artifactref) | 必填 |
| `timeout_seconds` | integer | 必填 |
| `kind` | `"package"` | 必填 |

拒绝额外字段。

区分字段 `kind=binary`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `version_directory` | string 或 null | 可省略 |
| `path` | string | 必填 |
| `artifact` | [ArtifactRef](api-types.md#crates_core_model_artifact_artifactref) | 必填 |
| `mode` | integer | 必填 |
| `kind` | `"binary"` | 必填 |

拒绝额外字段。

区分字段 `kind=file`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `path` | string | 必填 |
| `sha256` | string | 必填 |
| `mode` | integer | 必填 |
| `kind` | `"file"` | 必填 |

拒绝额外字段。

区分字段 `kind=service`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `executable_sha256` | string 或 null | 可省略 |
| `name` | string | 必填 |
| `running` | boolean | 必填 |
| `enabled` | boolean | 必填 |
| `kind` | `"service"` | 必填 |

拒绝额外字段。

区分字段 `kind=exec`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `fixed_environment` | map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | 可省略 |
| `resources` | string[] | 可省略 |
| `execution_profile` | string 或 null | 可省略 |
| `execution` | string 或 null | 可省略 |
| `origin` | [ScriptOrigin](api-types.md#crates_plugin_api_lib_scriptorigin) 或 null | 可省略 |
| `environment` | map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | 可省略 |
| `script_sha256` | string | 必填 |
| `timeout_seconds` | integer | 必填 |
| `kind` | `"exec"` | 必填 |

拒绝额外字段。

区分字段 `kind=reboot`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `requested` | boolean | 可省略 |
| `recovery_timeout_seconds` | integer | 必填 |
| `expected_kernel` | string 或 null | 可省略 |
| `kind` | `"reboot"` | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_phase"></a>

### Phase

`crates_core_model_domain_Phase`

`"prepare"`

`"rollout"`

`"verify"`

<a id="crates_core_model_domain_plan"></a>

### Plan

`crates_core_model_domain_Plan`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `format` | integer | 必填 |
| `input` | [PlanInput](api-types.md#crates_core_model_domain_planinput) | 必填 |
| `sha256` | string | 必填 |
| `work` | [Work](api-types.md#crates_core_model_domain_work)[] | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_planinput"></a>

### PlanInput

`crates_core_model_domain_PlanInput`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `inventory` | [Inventory](api-types.md#crates_core_orchestration_inventory_inventory) 或 null | 可省略 |
| `bundle` | map<string, [PackageFile](api-types.md#crates_core_orchestration_inventory_packagefile)> | 可省略 |
| `target` | [ExternalTarget](api-types.md#crates_core_model_target_externaltarget) 或 null | 可省略 |
| `download_parallel` | integer | 可省略；默认 `16` |
| `project` | string 或 null | 可省略 |
| `hosts` | [HostsSnapshot](api-types.md#crates_core_model_domain_hostssnapshot) 或 null | 可省略 |
| `playbook` | [Playbook](api-types.md#crates_core_model_domain_playbook) | 必填 |
| `batch_size` | integer | 必填 |
| `owner_uid` | string | 必填 |
| `parameters` | map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | 可省略 |
| `workflow_values` | map<string, map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)>> | 可省略 |

拒绝额外字段。

<a id="crates_core_model_domain_playbook"></a>

### Playbook

`crates_core_model_domain_Playbook`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `api_version` | string | 必填 |
| `name` | string | 必填 |
| `steps` | [Step](api-types.md#crates_core_model_domain_step)[] | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_scope"></a>

### Scope

`crates_core_model_domain_Scope`

`"once_per_run"`

`"once_per_batch"`

`"per_node"`

<a id="crates_core_model_domain_step"></a>

### Step

`crates_core_model_domain_Step`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `hosts` | string 或 null | 可省略 |
| `allow_empty` | boolean | 可省略 |
| `node_order` | string[] | 可省略 |
| `outputs` | map<string, [Specification](api-types.md#crates_core_model_outputs_specification)> | 可省略 |
| `inputs` | map<string, [Reference](api-types.md#crates_core_model_outputs_reference)> | 可省略 |
| `max_parallel` | integer 或 null | 可省略 |
| `id` | string | 必填 |
| `phase` | [Phase](api-types.md#crates_core_model_domain_phase) | 必填 |
| `scope` | [Scope](api-types.md#crates_core_model_domain_scope) | 必填 |
| `operation` | [Operation](api-types.md#crates_core_model_domain_operation) | 必填 |

拒绝额外字段。

<a id="crates_core_model_domain_work"></a>

### Work

`crates_core_model_domain_Work`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `outputs` | map<string, [Specification](api-types.md#crates_core_model_outputs_specification)> | 可省略 |
| `inputs` | map<string, [Reference](api-types.md#crates_core_model_outputs_reference)> | 可省略 |
| `input_sha256` | string 或 null | 可省略 |
| `id` | string | 必填 |
| `step_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `phase` | [Phase](api-types.md#crates_core_model_domain_phase) | 必填 |
| `batch` | integer 或 null | 可省略 |
| `wave` | integer | 必填 |
| `operation` | [Operation](api-types.md#crates_core_model_domain_operation) | 必填 |

拒绝额外字段。

<a id="crates_core_model_evidence_evidence"></a>

### Evidence

`crates_core_model_evidence_Evidence`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `plugin` | [Observation](api-types.md#crates_plugin_api_lib_observation) 或 null | 可省略 |
| `operation` | 任意 JSON | 必填 |
| `service_before` | [State](api-types.md#crates_core_model_service_state) 或 null | 可省略 |
| `service_after` | [State](api-types.md#crates_core_model_service_state) 或 null | 可省略 |
| `before` | [FileState](api-types.md#crates_core_model_evidence_filestate) 或 null | 可省略 |
| `after` | [FileState](api-types.md#crates_core_model_evidence_filestate) 或 null | 可省略 |
| `changed` | boolean | 必填 |
| `error` | string 或 null | 可省略 |
| `artifact_size` | integer 或 null | 可省略 |
| `package` | [Evidence](api-types.md#crates_core_model_package_evidence) 或 null | 可省略 |
| `reboot` | [Reboot](api-types.md#crates_core_model_host_reboot) 或 null | 可省略 |
| `health` | [Observation](api-types.md#crates_core_model_health_observation) 或 null | 可省略 |

拒绝额外字段。

<a id="crates_core_model_evidence_filestate"></a>

### FileState

`crates_core_model_evidence_FileState`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `bytes` | integer | 可省略 |
| `sha256` | string | 必填 |
| `mode` | integer | 必填 |

<a id="crates_core_model_execution_evidence"></a>

### Evidence

`crates_core_model_execution_Evidence`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | integer | 必填 |
| `gid` | integer | 必填 |
| `mode` | [Mode](api-types.md#crates_core_model_execution_mode) | 必填 |
| `unit` | string 或 null | 可省略 |

<a id="crates_core_model_execution_mode"></a>

### Mode

`crates_core_model_execution_Mode`

`"process"`

`"systemd"`

<a id="crates_core_model_health_observation"></a>

### Observation

`crates_core_model_health_Observation`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `status` | integer | 必填 |
| `body_sha256` | string | 必填 |
| `bytes` | integer | 必填 |

<a id="crates_core_model_host_reboot"></a>

### Reboot

`crates_core_model_host_Reboot`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `before_boot` | string | 必填 |
| `after_boot` | string 或 null | 可省略 |
| `kernel` | string 或 null | 可省略 |
| `requested_ms` | integer | 必填 |

<a id="crates_core_model_logs_logs"></a>

### Logs

`crates_core_model_logs_Logs`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `stdout` | [Stream](api-types.md#crates_core_model_logs_stream) | 必填 |
| `stderr` | [Stream](api-types.md#crates_core_model_logs_stream) | 必填 |

<a id="crates_core_model_logs_stream"></a>

### Stream

`crates_core_model_logs_Stream`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `stored_bytes` | integer | 必填 |
| `seen_bytes` | integer | 必填 |
| `sha256` | string | 必填 |
| `tail` | string | 必填 |
| `truncated` | boolean | 必填 |

<a id="crates_core_model_outputs_materialized"></a>

### Materialized

`crates_core_model_outputs_Materialized`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `work` | [Work](api-types.md#crates_core_model_domain_work) | 必填 |
| `input_sha256` | string | 必填 |
| `source_sha256` | map<string, string> | 必填 |

拒绝额外字段。

<a id="crates_core_model_outputs_publickey"></a>

### PublicKey

`crates_core_model_outputs_PublicKey`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `id` | string | 必填 |
| `public_key` | string | 必填 |

拒绝额外字段。

<a id="crates_core_model_outputs_reference"></a>

### Reference

`crates_core_model_outputs_Reference`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `step` | string | 必填 |
| `name` | string | 必填 |
| `node` | string 或 null | 可省略 |
| `batch` | integer 或 null | 可省略 |
| `pointer` | string 或 null | 可省略 |
| `collect` | boolean | 可省略 |

拒绝额外字段。

<a id="crates_core_model_outputs_sealedvalue"></a>

### SealedValue

`crates_core_model_outputs_SealedValue`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `key_id` | string | 必填 |
| `ephemeral_key` | string | 必填 |
| `nonce` | string | 必填 |
| `ciphertext` | string | 必填 |
| `plaintext_bytes` | integer | 必填 |

拒绝额外字段。

<a id="crates_core_model_outputs_specification"></a>

### Specification

`crates_core_model_outputs_Specification`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `type` | [ValueType](api-types.md#crates_core_model_outputs_valuetype) | 必填 |
| `secret` | boolean | 可省略 |
| `max_bytes` | integer | 可省略 |
| `ttl_seconds` | integer | 可省略 |
| `source` | string 或 null | 可省略 |

拒绝额外字段。

<a id="crates_core_model_outputs_storedvalue"></a>

### StoredValue

`crates_core_model_outputs_StoredValue`

区分字段 `kind=public`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `value` | 任意 JSON | 必填 |
| `kind` | `"public"` | 必填 |

拒绝额外字段。

区分字段 `kind=secret`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sealed` | [SealedValue](api-types.md#crates_core_model_outputs_sealedvalue) | 必填 |
| `kind` | `"secret"` | 必填 |

拒绝额外字段。

<a id="crates_core_model_outputs_valuetype"></a>

### ValueType

`crates_core_model_outputs_ValueType`

`"string"`

`"integer"`

`"number"`

`"boolean"`

`"object"`

`"array"`

`"null"`

<a id="crates_core_model_package_evidence"></a>

### Evidence

`crates_core_model_package_Evidence`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `before` | string 或 null | 可省略 |
| `after` | string 或 null | 可省略 |
| `transaction_exit` | integer 或 null | 可省略 |
| `diagnostic` | string | 必填 |

<a id="crates_core_model_package_manager"></a>

### Manager

`crates_core_model_package_Manager`

`"rpm"`

`"deb"`

<a id="crates_core_model_receipt_outcome"></a>

### Outcome

`crates_core_model_receipt_Outcome`

`"running"`

`"succeeded"`

`"failed"`

`"timed_out"`

`"canceled"`

`"needs_attention"`

<a id="crates_core_model_receipt_receipt"></a>

### Receipt

`crates_core_model_receipt_Receipt`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `outputs` | map<string, [StoredValue](api-types.md#crates_core_model_outputs_storedvalue)> | 可省略 |
| `diagnostic` | string 或 null | 可省略 |
| `execution` | [Evidence](api-types.md#crates_core_model_execution_evidence) 或 null | 可省略 |
| `logs` | [Logs](api-types.md#crates_core_model_logs_logs) 或 null | 可省略 |
| `native` | [Evidence](api-types.md#crates_core_model_evidence_evidence) 或 null | 可省略 |
| `operation_id` | string | 必填 |
| `script_sha256` | string | 必填 |
| `timeout_seconds` | integer | 必填 |
| `outcome` | [Outcome](api-types.md#crates_core_model_receipt_outcome) | 必填 |
| `exit_code` | integer 或 null | 可省略 |
| `boot_id` | string | 必填 |

<a id="crates_core_model_remote_logs_manifest"></a>

### Manifest

`crates_core_model_remote_logs_Manifest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `chunk_hashes` | string[] | 可省略 |
| `missing_ranges` | [integer, integer][] | 可省略 |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `grant` | string | 必填 |
| `store_id` | string | 必填 |
| `stream` | string | 必填 |
| `bytes` | integer | 必填 |
| `sha256` | string | 必填 |
| `chunks` | integer | 必填 |
| `truncated` | boolean | 必填 |
| `expires_ms` | integer | 必填 |

拒绝额外字段。

<a id="crates_core_model_service_state"></a>

### State

`crates_core_model_service_State`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `executable_sha256` | string 或 null | 可省略 |
| `main_pid` | integer 或 null | 可省略 |
| `load` | string | 必填 |
| `active` | string | 必填 |
| `enabled` | string | 必填 |
| `sub` | string | 必填 |

<a id="crates_core_model_target_externaltarget"></a>

### ExternalTarget

`crates_core_model_target_ExternalTarget`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `revision` | integer | 必填 |
| `project` | string | 可省略；默认 `"default"` |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | 必填 |
| `connection` | string | 必填 |
| `spec` | 任意 JSON | 必填 |

拒绝额外字段。

<a id="crates_core_model_telemetry_snapshot"></a>

### Snapshot

`crates_core_model_telemetry_Snapshot`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `collected_ms` | integer | 必填 |
| `data` | map<string, 任意 JSON> | 必填 |
| `errors` | map<string, string> | 可省略 |
| `truncated` | boolean | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_administration_administration"></a>

### Administration

`crates_core_orchestration_administration_Administration`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `secrets` | map<string, map<string, [SealedValue](api-types.md#crates_core_model_outputs_sealedvalue)>> | 可省略 |
| `artifacts` | map<string, integer> | 可省略 |
| `retired_node_groups` | map<string, [NodeGroup](api-types.md#crates_core_model_domain_nodegroup)> | 可省略 |
| `windows` | map<string, [MaintenanceWindow](api-types.md#crates_core_orchestration_administration_maintenancewindow)> | 可省略 |
| `owner_revisions` | map<string, integer> | 可省略 |
| `maintenance` | string[] | 必填 |
| `audit` | [Change](api-types.md#crates_core_orchestration_administration_change)[] | 必填 |

<a id="crates_core_orchestration_administration_change"></a>

### Change

`crates_core_orchestration_administration_Change`

区分字段 `action=secret`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `name` | string | 必填 |
| `version` | string | 必填 |
| `sealed` | [SealedValue](api-types.md#crates_core_model_outputs_sealedvalue) | 必填 |
| `action` | `"secret"` | 必填 |

拒绝额外字段。

区分字段 `action=artifact`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sha256` | string | 必填 |
| `size` | integer | 必填 |
| `action` | `"artifact"` | 必填 |

拒绝额外字段。

区分字段 `action=log_pruned`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `key` | string | 必填 |
| `store_id` | string | 必填 |
| `sha256` | string | 必填 |
| `action` | `"log_pruned"` | 必填 |

拒绝额外字段。

区分字段 `action=log_storage`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `store_id` | string | 必填 |
| `action` | `"log_storage"` | 必填 |

拒绝额外字段。

区分字段 `action=log_final`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `manifest` | [Manifest](api-types.md#crates_core_model_remote_logs_manifest) | 必填 |
| `action` | `"log_final"` | 必填 |

拒绝额外字段。

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

<a id="crates_core_orchestration_administration_maintenancewindow"></a>

### MaintenanceWindow

`crates_core_orchestration_administration_MaintenanceWindow`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `revision` | integer | 必填 |
| `node_uids` | string[] | 必填 |
| `start_ms` | integer | 必填 |
| `end_ms` | integer | 必填 |
| `enabled` | boolean | 必填 |

拒绝额外字段。

<a id="crates_core_orchestration_controlplane_catalog"></a>

### Catalog

`crates_core_orchestration_controlplane_Catalog`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `active` | string 或 null | 可省略 |
| `operations` | map<string, [Operation](api-types.md#crates_core_orchestration_controlplane_operation)> | 必填 |

<a id="crates_core_orchestration_controlplane_change"></a>

### Change

`crates_core_orchestration_controlplane_Change`

区分字段 `action=request`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `intent` | [Replace](api-types.md#crates_core_orchestration_controlplane_replace) | 必填 |
| `existing_fingerprints` | string[] | 必填 |
| `action` | `"request"` | 必填 |

拒绝额外字段。

区分字段 `action=observe`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `condition` | string | 必填 |
| `diagnostic` | string 或 null | 可省略 |
| `action` | `"observe"` | 必填 |

拒绝额外字段。

<a id="crates_core_orchestration_controlplane_operation"></a>

### Operation

`crates_core_orchestration_controlplane_Operation`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `intent` | [Replace](api-types.md#crates_core_orchestration_controlplane_replace) | 必填 |
| `condition` | string | 必填 |
| `diagnostic` | string 或 null | 可省略 |
| `revision` | integer | 必填 |

<a id="crates_core_orchestration_controlplane_replace"></a>

### Replace

`crates_core_orchestration_controlplane_Replace`

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

<a id="crates_core_orchestration_drift_policy"></a>

### Policy

`crates_core_orchestration_drift_Policy`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `configuration_revision` | integer | 可省略 |
| `configuration_evidence` | string 或 null | 可省略 |
| `interval_seconds` | integer 或 null | 可省略 |
| `last_started_ms` | integer | 必填 |
| `sequence` | integer | 必填 |
| `active_run` | string 或 null | 可省略 |
| `last_run` | string 或 null | 可省略 |
| `target_generation` | integer | 必填 |
| `observed_generation` | integer 或 null | 可省略 |
| `condition` | string | 必填 |

<a id="crates_core_orchestration_history_change"></a>

### Change

`crates_core_orchestration_history_Change`

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

<a id="crates_core_orchestration_history_history"></a>

### History

`crates_core_orchestration_history_History`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `records` | map<string, [Record](api-types.md#crates_core_orchestration_history_record)> | 必填 |
| `keep_runs` | integer | 必填 |
| `definition_hot_bytes` | integer | 必填 |
| `configuration_revision` | integer | 必填 |
| `audit` | 任意 JSON[] | 必填 |

<a id="crates_core_orchestration_history_kind"></a>

### Kind

`crates_core_orchestration_history_Kind`

`"run"`

`"definition"`

<a id="crates_core_orchestration_history_record"></a>

### Record

`crates_core_orchestration_history_Record`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `submitted_ms` | integer 或 null | 可省略 |
| `finished_ms` | integer 或 null | 可省略 |
| `kind` | [Kind](api-types.md#crates_core_orchestration_history_kind) | 必填 |
| `uid` | string | 必填 |
| `sha256` | string | 必填 |
| `bytes` | integer | 必填 |
| `revision` | integer | 必填 |
| `owner` | string | 必填 |
| `project` | string 或 null | 可省略 |
| `node_group_uid` | string 或 null | 可省略 |
| `target` | string | 必填 |
| `sequence` | integer | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) 或 null | 可省略 |
| `plan_sha256` | string | 必填 |
| `total` | integer | 必填 |
| `completed` | integer | 必填 |
| `archived_ms` | integer | 必填 |

<a id="crates_core_orchestration_inventory_host"></a>

### Host

`crates_core_orchestration_inventory_Host`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `address` | string | 必填 |
| `hostname` | string | 必填 |
| `values` | map<string, 任意 JSON> | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_inventory_inventory"></a>

### Inventory

`crates_core_orchestration_inventory_Inventory`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `nodes` | [Host](api-types.md#crates_core_orchestration_inventory_host)[] | 必填 |
| `groups` | map<string, string[]> | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_inventory_packagefile"></a>

### PackageFile

`crates_core_orchestration_inventory_PackageFile`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `sha256` | string | 必填 |
| `executable` | boolean | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_machine_types_attempt"></a>

### Attempt

`crates_core_orchestration_machine_types_Attempt`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `completed_ms` | integer 或 null | 可省略 |
| `materialized` | [Materialized](api-types.md#crates_core_model_outputs_materialized) 或 null | 可省略 |
| `authorized_ms` | integer 或 null | 可省略 |
| `reason` | string 或 null | 可省略 |
| `receipt` | [Receipt](api-types.md#crates_core_model_receipt_receipt) 或 null | 可省略 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `grant` | string 或 null | 可省略 |
| `boot_id` | string 或 null | 可省略 |
| `session_id` | string 或 null | 可省略 |
| `last_sequence` | integer | 必填 |
| `reports` | map<string, string> | 必填 |

<a id="crates_core_orchestration_machine_types_audithead"></a>

### AuditHead

`crates_core_orchestration_machine_types_AuditHead`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `count` | integer | 必填 |
| `sha256` | string | 必填 |

<a id="crates_core_orchestration_machine_types_command"></a>

### Command

`crates_core_orchestration_machine_types_Command`

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

<a id="crates_core_orchestration_machine_types_deployment"></a>

### Deployment

`crates_core_orchestration_machine_types_Deployment`

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

<a id="crates_core_orchestration_machine_types_nodeobservation"></a>

### NodeObservation

`crates_core_orchestration_machine_types_NodeObservation`

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
| `observed_ms` | integer | 必填 |

<a id="crates_core_orchestration_machine_types_reply"></a>

### Reply

`crates_core_orchestration_machine_types_Reply`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `ok` | boolean | 必填 |
| `code` | string | 必填 |
| `run_id` | string 或 null | 可省略 |
| `revision` | integer 或 null | 可省略 |

<a id="crates_core_orchestration_machine_types_resolution"></a>

### Resolution

`crates_core_orchestration_machine_types_Resolution`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `actor` | string | 必填 |
| `evidence` | string | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |

<a id="crates_core_orchestration_machine_types_run"></a>

### Run

`crates_core_orchestration_machine_types_Run`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) 或 null | 可省略 |
| `submitted_ms` | integer 或 null | 可省略 |
| `finished_ms` | integer 或 null | 可省略 |
| `artifact_nodes` | string[] 或 null | 可省略 |
| `preparing` | boolean | 可省略 |
| `prepared` | string[] | 可省略 |
| `preparation_errors` | map<string, string> | 可省略 |
| `sequence` | integer | 可省略 |
| `pause_requested` | boolean | 可省略 |
| `id` | string | 必填 |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `cancel_requested` | boolean | 必填 |
| `revision` | integer | 必填 |
| `attempts` | map<string, [Attempt](api-types.md#crates_core_orchestration_machine_types_attempt)> | 必填 |

<a id="crates_core_orchestration_machine_types_state"></a>

### State

`crates_core_orchestration_machine_types_State`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflows` | [Registry](api-types.md#crates_core_orchestration_workflow_registry) | 可省略 |
| `personnel` | [Registry](api-types.md#crates_core_identities_personnel_registry) | 可省略 |
| `log_pruned` | map<string, string> | 可省略 |
| `log_store_id` | string 或 null | 可省略 |
| `log_manifests` | map<string, [Manifest](api-types.md#crates_core_model_remote_logs_manifest)> | 可省略 |
| `cold_audit` | map<string, [AuditHead](api-types.md#crates_core_orchestration_machine_types_audithead)> | 可省略 |
| `retirement_proofs` | map<string, string> | 可省略 |
| `targets` | map<string, [ExternalTarget](api-types.md#crates_core_model_target_externaltarget)> | 可省略 |
| `history` | [History](api-types.md#crates_core_orchestration_history_history) | 可省略 |
| `controlplane` | [Catalog](api-types.md#crates_core_orchestration_controlplane_catalog) | 可省略 |
| `run_sequence` | integer | 可省略 |
| `registry` | [Registry](api-types.md#crates_core_identities_registration_registry) | 可省略 |
| `drift` | map<string, [Policy](api-types.md#crates_core_orchestration_drift_policy)> | 可省略 |
| `owner_projects` | map<string, string 或 null> | 可省略 |
| `administration` | [Administration](api-types.md#crates_core_orchestration_administration_administration) | 可省略 |
| `identities` | [Identities](api-types.md#crates_core_identities_identity_identities) | 可省略 |
| `deployments` | map<string, [Deployment](api-types.md#crates_core_orchestration_machine_types_deployment)> | 可省略 |
| `observations` | map<string, [NodeObservation](api-types.md#crates_core_orchestration_machine_types_nodeobservation)> | 可省略 |
| `scripts` | map<string, string> | 可省略 |
| `node_groups` | map<string, [NodeGroup](api-types.md#crates_core_model_domain_nodegroup)> | 必填 |
| `runs` | map<string, [Run](api-types.md#crates_core_orchestration_machine_types_run)> | 必填 |
| `execution_locks` | map<string, string> | 必填 |
| `resource_owners` | map<string, string> | 必填 |
| `idempotency` | map<string, [string, string]> | 必填 |
| `audit` | [Resolution](api-types.md#crates_core_orchestration_machine_types_resolution)[] | 必填 |

<a id="crates_core_orchestration_machine_types_status"></a>

### Status

`crates_core_orchestration_machine_types_Status`

`"pending"`

`"preparing"`

`"paused"`

`"authorized"`

`"running"`

`"cancel_requested"`

`"needs_attention"`

`"succeeded"`

`"failed"`

`"canceled"`

<a id="crates_core_orchestration_preparation_change"></a>

### Change

`crates_core_orchestration_preparation_Change`

区分字段 `action=prepared`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `sha256` | string | 必填 |
| `size` | integer | 必填 |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `action` | `"prepared"` | 必填 |

拒绝额外字段。

区分字段 `action=failed`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `work_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `reason` | string | 必填 |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) 或 null | 可省略 |
| `action` | `"failed"` | 必填 |

拒绝额外字段。

区分字段 `action=admit`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"admit"` | 必填 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_actor"></a>

### Actor

`crates_core_orchestration_workflow_Actor`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `fingerprint` | string | 必填 |
| `credential_uid` | string | 必填 |

<a id="crates_core_orchestration_workflow_binding"></a>

### Binding

`crates_core_orchestration_workflow_Binding`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `step_id` | string | 必填 |
| `path` | string | 必填 |
| `transform` | [BindingTransform](api-types.md#crates_core_orchestration_workflow_bindingtransform) 或 null | 可省略 |
| `allowed_json_paths` | string[] | 可省略 |
| `scalar_rules` | map<string, [ScalarRule](api-types.md#crates_core_orchestration_workflow_scalarrule)> | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_bindingtransform"></a>

### BindingTransform

`crates_core_orchestration_workflow_BindingTransform`

`"sha256"`

<a id="crates_core_orchestration_workflow_change"></a>

### Change

`crates_core_orchestration_workflow_Change`

区分字段 `action=register`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflow` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"register"` | 必填 |

拒绝额外字段。

区分字段 `action=enable`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `enabled` | boolean | 必填 |
| `expected_revision` | integer | 必填 |
| `action` | `"enable"` | 必填 |

拒绝额外字段。

区分字段 `action=execute`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) 或 null | 可省略 |
| `request` | [Request](api-types.md#crates_core_orchestration_workflow_request) | 必填 |
| `actor` | [Actor](api-types.md#crates_core_orchestration_workflow_actor) | 必填 |
| `now_ms` | integer | 必填 |
| `action` | `"execute"` | 必填 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_execution"></a>

### Execution

`crates_core_orchestration_workflow_Execution`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request` | [Request](api-types.md#crates_core_orchestration_workflow_request) | 必填 |
| `actor` | [Actor](api-types.md#crates_core_orchestration_workflow_actor) | 必填 |
| `parameters` | map<string, 任意 JSON> | 必填 |
| `submitted_ms` | integer | 必填 |
| `deployment_uid` | string 或 null | 可省略 |
| `system` | boolean | 可省略 |

<a id="crates_core_orchestration_workflow_parameter"></a>

### Parameter

`crates_core_orchestration_workflow_Parameter`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kind` | [ParameterType](api-types.md#crates_core_orchestration_workflow_parametertype) | 必填 |
| `default` | 任意 JSON 或 null | 可省略 |
| `choices` | 任意 JSON[] | 可省略 |
| `minimum` | number 或 null | 可省略 |
| `maximum` | number 或 null | 可省略 |
| `max_length` | integer 或 null | 可省略 |
| `bindings` | [Binding](api-types.md#crates_core_orchestration_workflow_binding)[] | 必填 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_parametertype"></a>

### ParameterType

`crates_core_orchestration_workflow_ParameterType`

`"string"`

`"integer"`

`"number"`

`"boolean"`

<a id="crates_core_orchestration_workflow_registry"></a>

### Registry

`crates_core_orchestration_workflow_Registry`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflows` | map<string, [Workflow](api-types.md#crates_core_orchestration_workflow_workflow)> | 必填 |
| `revisions` | map<string, [Revision](api-types.md#crates_core_orchestration_workflow_revision)> | 必填 |
| `executions` | map<string, [Execution](api-types.md#crates_core_orchestration_workflow_execution)> | 必填 |

<a id="crates_core_orchestration_workflow_request"></a>

### Request

`crates_core_orchestration_workflow_Request`

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

<a id="crates_core_orchestration_workflow_revision"></a>

### Revision

`crates_core_orchestration_workflow_Revision`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `inventory` | [Inventory](api-types.md#crates_core_orchestration_inventory_inventory) 或 null | 可省略 |
| `bundle` | map<string, [PackageFile](api-types.md#crates_core_orchestration_inventory_packagefile)> | 可省略 |
| `uid` | string | 必填 |
| `name` | string | 必填 |
| `playbook` | [Playbook](api-types.md#crates_core_model_domain_playbook) | 必填 |
| `targets` | map<string, [TargetPreset](api-types.md#crates_core_orchestration_workflow_targetpreset)> | 必填 |
| `fixed` | map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | 可省略 |
| `values` | map<string, 任意 JSON> | 可省略 |
| `immutable` | string[] | 可省略 |
| `node_values` | map<string, map<string, 任意 JSON>> | 可省略 |
| `parameters` | map<string, [Parameter](api-types.md#crates_core_orchestration_workflow_parameter)> | 可省略 |
| `definitions` | map<string, string> | 可省略 |
| `provenance` | map<string, string> | 可省略 |
| `batch_size` | integer | 可省略 |
| `download_parallel` | integer | 可省略 |
| `reconcile_seconds` | integer 或 null | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_scalarrule"></a>

### ScalarRule

`crates_core_orchestration_workflow_ScalarRule`

区分字段 `kind=integer`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `minimum` | integer | 必填 |
| `maximum` | integer | 必填 |
| `kind` | `"integer"` | 必填 |

拒绝额外字段。

区分字段 `kind=quantity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kind` | `"quantity"` | 必填 |

拒绝额外字段。

区分字段 `kind=duration`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kind` | `"duration"` | 必填 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_targetpreset"></a>

### TargetPreset

`crates_core_orchestration_workflow_TargetPreset`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `hosts` | string 或 null | 可省略 |
| `owner_uid` | string 或 null | 可省略 |
| `node_group_uid` | string 或 null | 可省略 |
| `external_target_uid` | string 或 null | 可省略 |

拒绝额外字段。

<a id="crates_core_orchestration_workflow_workflow"></a>

### Workflow

`crates_core_orchestration_workflow_Workflow`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `current` | string | 必填 |
| `revision` | integer | 必填 |
| `enabled` | boolean | 必填 |

<a id="crates_plugin_api_lib_call"></a>

### Call

`crates_plugin_api_lib_Call`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | 必填 |
| `action` | string | 必填 |
| `location` | [Location](api-types.md#crates_plugin_api_lib_location) | 必填 |
| `spec` | 任意 JSON | 必填 |
| `resources` | string[] | 必填 |
| `reconcilable` | boolean | 必填 |
| `timeout_seconds` | integer | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_lib_descriptor"></a>

### Descriptor

`crates_plugin_api_lib_Descriptor`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#crates_plugin_api_lib_identity) | 必填 |
| `location` | [Location](api-types.md#crates_plugin_api_lib_location) | 必填 |
| `actions` | string[] | 必填 |
| `description` | string | 必填 |

<a id="crates_plugin_api_lib_expansion"></a>

### Expansion

`crates_plugin_api_lib_Expansion`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | 必填 |
| `action` | string | 必填 |
| `steps` | 任意 JSON[] | 必填 |
| `definitions` | map<string, string> | 必填 |

<a id="crates_plugin_api_lib_identity"></a>

### Identity

`crates_plugin_api_lib_Identity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `id` | string | 必填 |
| `version` | string | 必填 |
| `contract` | integer | 必填 |
| `sha256` | string | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_lib_location"></a>

### Location

`crates_plugin_api_lib_Location`

`"node"`

`"controller"`

<a id="crates_plugin_api_lib_observation"></a>

### Observation

`crates_plugin_api_lib_Observation`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `owner` | string | 必填 |
| `fence` | integer | 必填 |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | 必填 |
| `call_sha256` | string | 必填 |
| `payload` | 任意 JSON | 必填 |
| `verified` | boolean | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_lib_scriptorigin"></a>

### ScriptOrigin

`crates_plugin_api_lib_ScriptOrigin`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | 必填 |
| `action` | string | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_variables_outputsecretreference"></a>

### OutputSecretReference

`crates_plugin_api_variables_OutputSecretReference`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `output_secret` | [OutputSecretSource](api-types.md#crates_plugin_api_variables_outputsecretsource) | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_variables_outputsecretsource"></a>

### OutputSecretSource

`crates_plugin_api_variables_OutputSecretSource`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `run_id` | string | 必填 |
| `step` | string | 必填 |
| `name` | string | 必填 |
| `work_ids` | string[] | 必填 |
| `pointer` | string 或 null | 可省略 |
| `collect` | boolean | 必填 |
| `sha256` | string | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_variables_reference"></a>

### Reference

`crates_plugin_api_variables_Reference`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `secret` | string | 必填 |
| `version` | string | 必填 |

拒绝额外字段。

<a id="crates_plugin_api_variables_resolved"></a>

### Resolved

`crates_plugin_api_variables_Resolved`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `values` | map<string, string> | 必填 |
| `redactions` | integer[][] | 必填 |

<a id="crates_plugin_api_variables_value"></a>

### Value

`crates_plugin_api_variables_Value`

string

[Reference](api-types.md#crates_plugin_api_variables_reference)

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `secret` | string | 必填 |
| `version` | string | 必填 |

拒绝额外字段。

[OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `output_secret` | [OutputSecretSource](api-types.md#crates_plugin_api_variables_outputsecretsource) | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_access_accesscheck"></a>

### AccessCheck

`crates_runtime_controller_server_access_AccessCheck`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `action` | [Action](api-types.md#crates_core_identities_access_action) | 必填 |
| `workflow` | string 或 null | 可省略 |
| `preset` | string 或 null | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_artifacts_artifactupload"></a>

### ArtifactUpload

`crates_runtime_controller_server_artifacts_ArtifactUpload`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `size` | integer | 必填 |
| `offset` | integer | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_identity_activaterequest"></a>

### ActivateRequest

`crates_runtime_controller_server_identity_ActivateRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_identity_bootstraprequest"></a>

### BootstrapRequest

`crates_runtime_controller_server_identity_BootstrapRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `id` | string | 必填 |
| `node_uid` | string | 必填 |
| `token_sha256` | string | 必填 |
| `csr_pem` | string | 必填 |
| `ttl_seconds` | integer | 必填 |
| `recover` | boolean | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_identity_enrollrequest"></a>

### EnrollRequest

`crates_runtime_controller_server_identity_EnrollRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string | 必填 |
| `grant_id` | string | 必填 |
| `node_uid` | string | 必填 |
| `hardware_sha256` | string | 必填 |
| `token_sha256` | string | 必填 |
| `csr_pem` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_identity_renewalrequest"></a>

### RenewalRequest

`crates_runtime_controller_server_identity_RenewalRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `request_id` | string | 必填 |
| `csr_pem` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_identity_retirerequest"></a>

### RetireRequest

`crates_runtime_controller_server_identity_RetireRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `node_uid` | string | 必填 |
| `expected_fingerprint` | string | 必填 |
| `evidence` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_outputs_inputsquery"></a>

### InputsQuery

`crates_runtime_controller_server_outputs_InputsQuery`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_product_auditquery"></a>

### AuditQuery

`crates_runtime_controller_server_product_AuditQuery`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `after` | integer | 可省略 |
| `limit` | integer | 可省略；默认 `50` |
| `stream` | string | 可省略；默认 `"commands"` |
| `view` | string | 可省略；默认 `"all"` |

拒绝额外字段。

<a id="crates_runtime_controller_server_queries_pagequery"></a>

### PageQuery

`crates_runtime_controller_server_queries_PageQuery`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `search` | string 或 null | 可省略 |
| `workflow` | string 或 null | 可省略 |
| `preset` | string 或 null | 可省略 |
| `actor` | string 或 null | 可省略 |
| `after` | string 或 null | 可省略 |
| `limit` | integer | 可省略；默认 `50` |
| `status` | string 或 null | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_remote_logs_chunk"></a>

### Chunk

`crates_runtime_controller_server_remote_logs_Chunk`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `grant` | string 或 null | 可省略 |
| `offset` | integer | 必填 |
| `data` | string | 必填 |
| `sha256` | string | 必填 |
| `complete` | boolean | 可省略 |
| `missing` | boolean | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_remote_logs_logquery"></a>

### LogQuery

`crates_runtime_controller_server_remote_logs_LogQuery`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `offset` | integer | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_runs_heartbeat"></a>

### Heartbeat

`crates_runtime_controller_server_runs_Heartbeat`

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

<a id="crates_runtime_controller_server_runs_pauserequest"></a>

### PauseRequest

`crates_runtime_controller_server_runs_PauseRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `paused` | boolean | 必填 |
| `expected_revision` | integer | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_runs_poll"></a>

### Poll

`crates_runtime_controller_server_runs_Poll`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `after` | integer 或 null | 可省略 |
| `wait_ms` | integer | 可省略 |

拒绝额外字段。

<a id="crates_runtime_controller_server_runs_resolverequest"></a>

### ResolveRequest

`crates_runtime_controller_server_runs_ResolveRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | 必填 |
| `actor` | string | 必填 |
| `evidence` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_runs_uncertainrequest"></a>

### UncertainRequest

`crates_runtime_controller_server_runs_UncertainRequest`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `work_id` | string | 必填 |
| `expected_revision` | integer | 必填 |
| `reason` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_secrets_registersecret"></a>

### RegisterSecret

`crates_runtime_controller_server_secrets_RegisterSecret`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `name` | string | 必填 |
| `version` | string | 必填 |
| `value` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_workflows_enableworkflow"></a>

### EnableWorkflow

`crates_runtime_controller_server_workflows_EnableWorkflow`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `enabled` | boolean | 必填 |
| `expected_revision` | integer | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_workflows_registerworkflow"></a>

### RegisterWorkflow

`crates_runtime_controller_server_workflows_RegisterWorkflow`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflow` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | 必填 |
| `expected_revision` | integer | 必填 |

拒绝额外字段。

<a id="crates_runtime_controller_server_workflows_workflowpreview"></a>

### WorkflowPreview

`crates_runtime_controller_server_workflows_WorkflowPreview`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `workflow_revision` | string | 必填 |
| `target` | string | 必填 |
| `parameters` | map<string, 任意 JSON> | 可省略 |
| `request_id` | string | 必填 |

拒绝额外字段。

<a id="crates_runtime_storage_store_machine"></a>

### Machine

`crates_runtime_storage_store_Machine`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `last_applied` | [RaftLogId](api-types.md#raftlogid) 或 null | 可省略 |
| `membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | 必填 |
| `state` | [State](api-types.md#crates_core_orchestration_machine_types_state) | 必填 |

<a id="crates_runtime_storage_store_position"></a>

### Position

`crates_runtime_storage_store_Position`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `last_applied` | [RaftLogId](api-types.md#raftlogid) 或 null | 可省略 |
| `membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | 必填 |

<a id="plugins_docker_container_credential"></a>

### Credential

`plugins_docker_container_Credential`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `registry` | string | 必填 |
| `directory` | string | 必填 |
| `projects` | string[] | 可省略 |

拒绝额外字段。

<a id="plugins_docker_container_mount"></a>

### Mount

`plugins_docker_container_Mount`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `source` | string | 必填 |
| `target` | string | 必填 |
| `read_only` | boolean | 必填 |

拒绝额外字段。

<a id="plugins_docker_container_observation"></a>

### Observation

`plugins_docker_container_Observation`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `container_id` | string | 必填 |
| `image_id` | string | 必填 |
| `image_digest` | string | 必填 |
| `platform` | string | 必填 |
| `owner` | string | 必填 |
| `configuration_sha256` | string | 必填 |
| `healthy` | boolean | 必填 |

<a id="plugins_docker_container_port"></a>

### Port

`plugins_docker_container_Port`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `host` | integer | 必填 |
| `container` | integer | 必填 |
| `address` | string | 可省略；默认 `"127.0.0.1"` |

拒绝额外字段。

<a id="plugins_docker_container_rules"></a>

### Rules

`plugins_docker_container_Rules`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `names` | string[] | 必填 |
| `registries` | string[] | 必填 |
| `volume_roots` | string[] | 可省略 |
| `credentials` | map<string, [Credential](api-types.md#plugins_docker_container_credential)> | 可省略 |
| `allow_root_user` | boolean | 可省略 |

拒绝额外字段。

<a id="plugins_docker_container_spec"></a>

### Spec

`plugins_docker_container_Spec`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `name` | string | 必填 |
| `image` | string | 必填 |
| `platform` | string | 必填 |
| `command` | string[] | 必填 |
| `environment` | map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | 可省略 |
| `ports` | [Port](api-types.md#plugins_docker_container_port)[] | 可省略 |
| `mounts` | [Mount](api-types.md#plugins_docker_container_mount)[] | 可省略 |
| `healthcheck` | string | 必填 |
| `readiness_timeout_seconds` | integer | 必填 |
| `user_id` | integer | 可省略；默认 `65534` |
| `memory_bytes` | integer | 可省略；默认 `268435456` |
| `cpu_millis` | integer | 可省略；默认 `1000` |
| `credential_ref` | [Reference](api-types.md#crates_plugin_api_variables_reference) 或 null | 可省略 |

拒绝额外字段。

<a id="plugins_kubeadm_lib_input"></a>

### Input

`plugins_kubeadm_lib_Input`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `version` | string | 必填 |
| `environment` | map<string, string 或 object 或 [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | 必填 |
| `execution` | string | 必填 |
| `timeout_seconds` | integer | 必填 |

拒绝额外字段。

<a id="plugins_kubernetes_kube_api_connection"></a>

### Connection

`plugins_kubernetes_kube_api_Connection`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `kubeconfig` | string | 必填 |
| `context` | string 或 null | 可省略 |
| `projects` | string[] | 可省略 |
| `namespaces` | string[] | 必填 |
| `allow_cluster_resources` | boolean | 可省略 |
| `allow_privileged_workloads` | boolean | 可省略 |

拒绝额外字段。

<a id="plugins_kubernetes_kube_api_identity"></a>

### Identity

`plugins_kubernetes_kube_api_Identity`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `cluster_uid` | string | 必填 |
| `namespace_uid` | string | 必填 |
| `version` | string | 必填 |

<a id="plugins_kubernetes_kube_resource_evidence"></a>

### Evidence

`plugins_kubernetes_kube_resource_Evidence`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `uid` | string | 必填 |
| `resource_version` | string | 必填 |
| `generation` | integer | 必填 |
| `observed_generation` | integer | 必填 |
| `ready` | boolean | 必填 |

<a id="plugins_kubernetes_kube_resource_intent"></a>

### Intent

`plugins_kubernetes_kube_resource_Intent`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `owner` | string | 必填 |
| `fence` | integer | 必填 |
| `namespace` | string | 必填 |
| `object` | 任意 JSON | 必填 |

<a id="plugins_kubernetes_lib_evidence"></a>

### Evidence

`plugins_kubernetes_lib_Evidence`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#plugins_kubernetes_kube_api_identity) | 必填 |
| `fence` | integer | 必填 |
| `resources` | [Evidence](api-types.md#plugins_kubernetes_kube_resource_evidence)[] | 必填 |

拒绝额外字段。

<a id="plugins_kubernetes_lib_spec"></a>

### Spec

`plugins_kubernetes_lib_Spec`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `target` | [Target](api-types.md#plugins_kubernetes_lib_target) | 必填 |
| `objects` | 任意 JSON[] | 必填 |
| `timeout_seconds` | integer | 必填 |

拒绝额外字段。

<a id="plugins_kubernetes_lib_target"></a>

### Target

`plugins_kubernetes_lib_Target`

| 字段 | JSON 类型 | 必填 / 默认 |
| --- | --- | --- |
| `namespace` | string | 必填 |
| `identity` | [Identity](api-types.md#plugins_kubernetes_kube_api_identity) | 必填 |

拒绝额外字段。

