# Converyn API type reference

[English](../../en/reference/api-types.md) | [简体中文](../../zh-CN/reference/api-types.md)

Generated from OpenAPI. Types are identified by source module and name, with fields, JSON types, required values and defaults. See the [API reference](api-reference.md) for route authorization.

<a id="apierror"></a>

### ApiError

`ApiError`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `code` | string | required |
| `message` | string | required |
| `retryable` | boolean | required |
| `request_id` | string | required |

<a id="logstreamerror"></a>

### LogStreamError

`LogStreamError`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `status` | integer | required |
| `code` | string | required |
| `message` | string | required |
| `retryable` | boolean | required |
| `request_id` | string | required |

<a id="page"></a>

### Page

`Page`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `items` | object[] | required |
| `next_cursor` | string or null | required |

<a id="raftappendentriesrequest"></a>

### RaftAppendEntriesRequest

`RaftAppendEntriesRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `prev_log_id` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `entries` | [RaftEntry](api-types.md#raftentry)[] | required |
| `leader_commit` | [RaftLogId](api-types.md#raftlogid) or null | required |

<a id="raftentry"></a>

### RaftEntry

`RaftEntry`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `log_id` | [RaftLogId](api-types.md#raftlogid) | required |
| `payload` | `"Blank"` or object or object | required |

<a id="raftinstallsnapshotrequest"></a>

### RaftInstallSnapshotRequest

`RaftInstallSnapshotRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `meta` | [RaftSnapshotMeta](api-types.md#raftsnapshotmeta) | required |
| `offset` | integer | required |
| `data` | integer[] | required |
| `done` | boolean | required |

<a id="raftleaderid"></a>

### RaftLeaderId

`RaftLeaderId`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `term` | integer | required |
| `node_id` | integer | required |

<a id="raftlogid"></a>

### RaftLogId

`RaftLogId`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `leader_id` | [RaftLeaderId](api-types.md#raftleaderid) | required |
| `index` | integer | required |

<a id="raftmembership"></a>

### RaftMembership

`RaftMembership`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `configs` | integer[][] | required |
| `nodes` | map<string, object> | required |

<a id="raftmetrics"></a>

### RaftMetrics

`RaftMetrics`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `running_state` | object or object | required |
| `id` | integer | required |
| `current_term` | integer | required |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `last_log_index` | integer or null | required |
| `last_applied` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `snapshot` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `purged` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `state` | string | required |
| `current_leader` | integer or null | required |
| `millis_since_quorum_ack` | integer or null | required |
| `membership_config` | [RaftStoredMembership](api-types.md#raftstoredmembership) | required |
| `replication` | map<string, [RaftLogId](api-types.md#raftlogid) or null> or null | required |

<a id="raftsnapshotmeta"></a>

### RaftSnapshotMeta

`RaftSnapshotMeta`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `last_log_id` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `last_membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | required |
| `snapshot_id` | string | required |

<a id="raftstoredmembership"></a>

### RaftStoredMembership

`RaftStoredMembership`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `log_id` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `membership` | [RaftMembership](api-types.md#raftmembership) | required |

<a id="raftvote"></a>

### RaftVote

`RaftVote`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `leader_id` | [RaftLeaderId](api-types.md#raftleaderid) | required |
| `committed` | boolean | required |

<a id="raftvoterequest"></a>

### RaftVoteRequest

`RaftVoteRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `last_log_id` | [RaftLogId](api-types.md#raftlogid) or null | required |

<a id="crates_core_identities_access_access"></a>

### Access

`crates_core_identities_access_Access`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `revision` | integer | required |
| `groups` | map<string, [Group](api-types.md#crates_core_identities_access_group)> | required |
| `bindings` | map<string, [Binding](api-types.md#crates_core_identities_access_binding)> | required |

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

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `subject` | [Subject](api-types.md#crates_core_identities_access_subject) | required |
| `role` | [Role](api-types.md#crates_core_identities_access_role) | required |
| `workflow` | string or null | optional |
| `presets` | string[] | optional |

Additional fields are rejected.

<a id="crates_core_identities_access_change"></a>

### Change

`crates_core_identities_access_Change`

Discriminator `action=put_group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `group` | [Group](api-types.md#crates_core_identities_access_group) | required |
| `expected_revision` | integer | required |
| `action` | `"put_group"` | required |

Additional fields are rejected.

Discriminator `action=delete_group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `expected_revision` | integer | required |
| `action` | `"delete_group"` | required |

Additional fields are rejected.

Discriminator `action=put_binding`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `binding` | [Binding](api-types.md#crates_core_identities_access_binding) | required |
| `expected_revision` | integer | required |
| `action` | `"put_binding"` | required |

Additional fields are rejected.

Discriminator `action=delete_binding`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `expected_revision` | integer | required |
| `action` | `"delete_binding"` | required |

Additional fields are rejected.

<a id="crates_core_identities_access_group"></a>

### Group

`crates_core_identities_access_Group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `name` | string | required |
| `members` | string[] | required |

Additional fields are rejected.

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

Discriminator `kind=person`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kind` | `"person"` | required |
| `uid` | string | required |

Additional fields are rejected.

Discriminator `kind=group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kind` | `"group"` | required |
| `uid` | string | required |

Additional fields are rejected.

<a id="crates_core_identities_certificate_signed"></a>

### Signed

`crates_core_identities_certificate_Signed`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `certificate_pem` | string | required |
| `fingerprint` | string | required |
| `expires_ms` | integer | required |

<a id="crates_core_identities_identity_binding"></a>

### Binding

`crates_core_identities_identity_Binding`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `revision` | integer | required |
| `fingerprints` | string[] | required |

<a id="crates_core_identities_identity_change"></a>

### Change

`crates_core_identities_identity_Change`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `principal` | string | required |
| `expected_revision` | integer | required |
| `fingerprints` | string[] | required |
| `evidence` | string | required |

<a id="crates_core_identities_identity_identities"></a>

### Identities

`crates_core_identities_identity_Identities`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `bindings` | map<string, [Binding](api-types.md#crates_core_identities_identity_binding)> | required |
| `audit` | [Change](api-types.md#crates_core_identities_identity_change)[] | required |

<a id="crates_core_identities_personnel_change"></a>

### Change

`crates_core_identities_personnel_Change`

Discriminator `action=add_identity`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) | required |
| `action` | `"add_identity"` | required |

Additional fields are rejected.

Discriminator `action=create`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `super_admin` | boolean | optional |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) | required |
| `credential` | [Credential](api-types.md#crates_core_identities_personnel_credential) | required |
| `request_id` | string | required |
| `now_ms` | integer | required |
| `action` | `"create"` | required |

Additional fields are rejected.

Discriminator `action=update`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `name` | string | required |
| `disabled` | boolean | required |
| `expected_revision` | integer | required |
| `action` | `"update"` | required |

Additional fields are rejected.

Discriminator `action=issue`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `credential` | [Credential](api-types.md#crates_core_identities_personnel_credential) | required |
| `expected_revision` | integer | required |
| `rotate` | boolean | required |
| `overlap_seconds` | integer | required |
| `request_id` | string | required |
| `now_ms` | integer | required |
| `action` | `"issue"` | required |

Additional fields are rejected.

Discriminator `action=revoke`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `expected_revision` | integer | required |
| `now_ms` | integer | required |
| `action` | `"revoke"` | required |

Additional fields are rejected.

<a id="crates_core_identities_personnel_credential"></a>

### Credential

`crates_core_identities_personnel_Credential`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `identity_uid` | string | required |
| `certificate_pem` | string | required |
| `fingerprint` | string | required |
| `token_sha256` | string | required |
| `not_before_ms` | integer | required |
| `expires_ms` | integer | required |
| `revoked_ms` | integer or null | optional |
| `overlap_until_ms` | integer or null | optional |

Additional fields are rejected.

<a id="crates_core_identities_personnel_identity"></a>

### Identity

`crates_core_identities_personnel_Identity`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `name` | string | required |
| `disabled` | boolean | required |
| `revision` | integer | required |

Additional fields are rejected.

<a id="crates_core_identities_personnel_registry"></a>

### Registry

`crates_core_identities_personnel_Registry`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `access` | [Access](api-types.md#crates_core_identities_access_access) | optional |
| `identities` | map<string, [Identity](api-types.md#crates_core_identities_personnel_identity)> | required |
| `credentials` | map<string, [Credential](api-types.md#crates_core_identities_personnel_credential)> | required |
| `operations` | map<string, string> | required |

<a id="crates_core_identities_registration_authorization"></a>

### Authorization

`crates_core_identities_registration_Authorization`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string or null | optional |
| `observed_ms` | integer or null | optional |
| `token_sha256` | string or null | optional |
| `principal` | string | required |
| `fingerprint` | string | required |

<a id="crates_core_identities_registration_bootstrap"></a>

### Bootstrap

`crates_core_identities_registration_Bootstrap`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `id` | string | required |
| `node_uid` | string | required |
| `token_sha256` | string | required |
| `csr_sha256` | string | required |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) | required |
| `expires_ms` | integer | required |
| `recover` | boolean | required |
| `consumed` | string or null | optional |
| `enrolled_certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) or null | optional |

<a id="crates_core_identities_registration_change"></a>

### Change

`crates_core_identities_registration_Change`

Discriminator `action=bootstrap`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant` | [Bootstrap](api-types.md#crates_core_identities_registration_bootstrap) | required |
| `now_ms` | integer | required |
| `action` | `"bootstrap"` | required |

Additional fields are rejected.

Discriminator `action=enroll`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant_id` | string | required |
| `bootstrap_sha256` | string | required |
| `bootstrap_fingerprint` | string | required |
| `node` | [Node](api-types.md#crates_core_identities_registration_node) | required |
| `now_ms` | integer | required |
| `action` | `"enroll"` | required |

Additional fields are rejected.

Discriminator `action=renew`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_uid` | string | required |
| `pending` | [PendingRenewal](api-types.md#crates_core_identities_registration_pendingrenewal) | required |
| `current_fingerprint` | string | required |
| `now_ms` | integer | required |
| `action` | `"renew"` | required |

Additional fields are rejected.

Discriminator `action=activate`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_uid` | string | required |
| `request_id` | string | required |
| `fingerprint` | string | required |
| `now_ms` | integer | required |
| `action` | `"activate"` | required |

Additional fields are rejected.

Discriminator `action=retire`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_uid` | string | required |
| `expected_fingerprint` | string | required |
| `evidence` | string | required |
| `action` | `"retire"` | required |

Additional fields are rejected.

Discriminator `action=expire_bootstrap`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `now_ms` | integer | required |
| `action` | `"expire_bootstrap"` | required |

Additional fields are rejected.

<a id="crates_core_identities_registration_node"></a>

### Node

`crates_core_identities_registration_Node`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `hardware_sha256` | string | required |
| `token_sha256` | string | required |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) | required |
| `enrollment_id` | string | required |
| `enrollment_digest` | string | required |
| `pending` | [PendingRenewal](api-types.md#crates_core_identities_registration_pendingrenewal) or null | optional |
| `retired` | boolean | required |

<a id="crates_core_identities_registration_pendingrenewal"></a>

### PendingRenewal

`crates_core_identities_registration_PendingRenewal`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `id` | string | required |
| `csr_sha256` | string | required |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) | required |

<a id="crates_core_identities_registration_proof"></a>

### Proof

`crates_core_identities_registration_Proof`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `principal` | string | required |
| `fingerprint` | string | required |
| `token_sha256` | string | required |
| `hardware_sha256` | string or null | optional |
| `session` | string or null | optional |

<a id="crates_core_identities_registration_registry"></a>

### Registry

`crates_core_identities_registration_Registry`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `bootstraps` | map<string, [Bootstrap](api-types.md#crates_core_identities_registration_bootstrap)> | required |
| `nodes` | map<string, [Node](api-types.md#crates_core_identities_registration_node)> | required |
| `token_index` | map<string, string> | required |
| `audit` | any JSON[] | required |

<a id="crates_core_model_artifact_artifactref"></a>

### ArtifactRef

`crates_core_model_artifact_ArtifactRef`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sha256` | string | required |
| `size` | integer | required |
| `locations` | [Location](api-types.md#crates_core_model_artifact_location)[] | optional |

Additional fields are rejected.

<a id="crates_core_model_artifact_location"></a>

### Location

`crates_core_model_artifact_Location`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `source` | string | required |
| `path` | string | required |

Additional fields are rejected.

<a id="crates_core_model_contracts_outcome"></a>

### Outcome

`crates_core_model_contracts_Outcome`

Discriminator `result=value`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `result` | `"value"` | required |
| `value` | any JSON | required |

Additional fields are rejected.

Discriminator `result=error`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `result` | `"error"` | required |
| `value` | string | required |

Additional fields are rejected.

<a id="crates_core_model_domain_hostssnapshot"></a>

### HostsSnapshot

`crates_core_model_domain_HostsSnapshot`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_group_uid` | string | required |
| `revision` | integer | required |
| `node_uids` | string[] | required |

Additional fields are rejected.

<a id="crates_core_model_domain_nodegroup"></a>

### NodeGroup

`crates_core_model_domain_NodeGroup`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `name` | string | required |
| `revision` | integer | required |
| `node_uids` | string[] | required |

Additional fields are rejected.

<a id="crates_core_model_domain_operation"></a>

### Operation

`crates_core_model_domain_Operation`

Discriminator `kind=plugin`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `call` | [Call](api-types.md#crates_plugin_api_lib_call) | required |
| `kind` | `"plugin"` | required |

Additional fields are rejected.

Discriminator `kind=http_check`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `url` | string | required |
| `expected_status` | integer | required |
| `body_sha256` | string or null | optional |
| `timeout_seconds` | integer | required |
| `kind` | `"http_check"` | required |

Additional fields are rejected.

Discriminator `kind=package`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `install_only` | boolean | optional |
| `manager` | [Manager](api-types.md#crates_core_model_package_manager) | required |
| `name` | string | required |
| `version` | string | required |
| `artifact` | [ArtifactRef](api-types.md#crates_core_model_artifact_artifactref) | required |
| `timeout_seconds` | integer | required |
| `kind` | `"package"` | required |

Additional fields are rejected.

Discriminator `kind=binary`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `version_directory` | string or null | optional |
| `path` | string | required |
| `artifact` | [ArtifactRef](api-types.md#crates_core_model_artifact_artifactref) | required |
| `mode` | integer | required |
| `kind` | `"binary"` | required |

Additional fields are rejected.

Discriminator `kind=file`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `path` | string | required |
| `sha256` | string | required |
| `mode` | integer | required |
| `kind` | `"file"` | required |

Additional fields are rejected.

Discriminator `kind=service`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `executable_sha256` | string or null | optional |
| `name` | string | required |
| `running` | boolean | required |
| `enabled` | boolean | required |
| `kind` | `"service"` | required |

Additional fields are rejected.

Discriminator `kind=exec`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `fixed_environment` | map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | optional |
| `resources` | string[] | optional |
| `execution_profile` | string or null | optional |
| `execution` | string or null | optional |
| `origin` | [ScriptOrigin](api-types.md#crates_plugin_api_lib_scriptorigin) or null | optional |
| `environment` | map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | optional |
| `script_sha256` | string | required |
| `timeout_seconds` | integer | required |
| `kind` | `"exec"` | required |

Additional fields are rejected.

Discriminator `kind=reboot`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `requested` | boolean | optional |
| `recovery_timeout_seconds` | integer | required |
| `expected_kernel` | string or null | optional |
| `kind` | `"reboot"` | required |

Additional fields are rejected.

<a id="crates_core_model_domain_phase"></a>

### Phase

`crates_core_model_domain_Phase`

`"prepare"`

`"rollout"`

`"verify"`

<a id="crates_core_model_domain_plan"></a>

### Plan

`crates_core_model_domain_Plan`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `format` | integer | required |
| `input` | [PlanInput](api-types.md#crates_core_model_domain_planinput) | required |
| `sha256` | string | required |
| `work` | [Work](api-types.md#crates_core_model_domain_work)[] | required |

Additional fields are rejected.

<a id="crates_core_model_domain_planinput"></a>

### PlanInput

`crates_core_model_domain_PlanInput`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `inventory` | [Inventory](api-types.md#crates_core_orchestration_inventory_inventory) or null | optional |
| `bundle` | map<string, [PackageFile](api-types.md#crates_core_orchestration_inventory_packagefile)> | optional |
| `target` | [ExternalTarget](api-types.md#crates_core_model_target_externaltarget) or null | optional |
| `download_parallel` | integer | optional; default `16` |
| `project` | string or null | optional |
| `hosts` | [HostsSnapshot](api-types.md#crates_core_model_domain_hostssnapshot) or null | optional |
| `playbook` | [Playbook](api-types.md#crates_core_model_domain_playbook) | required |
| `batch_size` | integer | required |
| `owner_uid` | string | required |
| `parameters` | map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | optional |
| `workflow_values` | map<string, map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)>> | optional |

Additional fields are rejected.

<a id="crates_core_model_domain_playbook"></a>

### Playbook

`crates_core_model_domain_Playbook`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `api_version` | string | required |
| `name` | string | required |
| `steps` | [Step](api-types.md#crates_core_model_domain_step)[] | required |

Additional fields are rejected.

<a id="crates_core_model_domain_scope"></a>

### Scope

`crates_core_model_domain_Scope`

`"once_per_run"`

`"once_per_batch"`

`"per_node"`

<a id="crates_core_model_domain_step"></a>

### Step

`crates_core_model_domain_Step`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `hosts` | string or null | optional |
| `allow_empty` | boolean | optional |
| `node_order` | string[] | optional |
| `outputs` | map<string, [Specification](api-types.md#crates_core_model_outputs_specification)> | optional |
| `inputs` | map<string, [Reference](api-types.md#crates_core_model_outputs_reference)> | optional |
| `max_parallel` | integer or null | optional |
| `id` | string | required |
| `phase` | [Phase](api-types.md#crates_core_model_domain_phase) | required |
| `scope` | [Scope](api-types.md#crates_core_model_domain_scope) | required |
| `operation` | [Operation](api-types.md#crates_core_model_domain_operation) | required |

Additional fields are rejected.

<a id="crates_core_model_domain_work"></a>

### Work

`crates_core_model_domain_Work`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `outputs` | map<string, [Specification](api-types.md#crates_core_model_outputs_specification)> | optional |
| `inputs` | map<string, [Reference](api-types.md#crates_core_model_outputs_reference)> | optional |
| `input_sha256` | string or null | optional |
| `id` | string | required |
| `step_id` | string | required |
| `node_uid` | string | required |
| `phase` | [Phase](api-types.md#crates_core_model_domain_phase) | required |
| `batch` | integer or null | optional |
| `wave` | integer | required |
| `operation` | [Operation](api-types.md#crates_core_model_domain_operation) | required |

Additional fields are rejected.

<a id="crates_core_model_evidence_evidence"></a>

### Evidence

`crates_core_model_evidence_Evidence`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `plugin` | [Observation](api-types.md#crates_plugin_api_lib_observation) or null | optional |
| `operation` | any JSON | required |
| `service_before` | [State](api-types.md#crates_core_model_service_state) or null | optional |
| `service_after` | [State](api-types.md#crates_core_model_service_state) or null | optional |
| `before` | [FileState](api-types.md#crates_core_model_evidence_filestate) or null | optional |
| `after` | [FileState](api-types.md#crates_core_model_evidence_filestate) or null | optional |
| `changed` | boolean | required |
| `error` | string or null | optional |
| `artifact_size` | integer or null | optional |
| `package` | [Evidence](api-types.md#crates_core_model_package_evidence) or null | optional |
| `reboot` | [Reboot](api-types.md#crates_core_model_host_reboot) or null | optional |
| `health` | [Observation](api-types.md#crates_core_model_health_observation) or null | optional |

Additional fields are rejected.

<a id="crates_core_model_evidence_filestate"></a>

### FileState

`crates_core_model_evidence_FileState`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `bytes` | integer | optional |
| `sha256` | string | required |
| `mode` | integer | required |

<a id="crates_core_model_execution_evidence"></a>

### Evidence

`crates_core_model_execution_Evidence`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | integer | required |
| `gid` | integer | required |
| `mode` | [Mode](api-types.md#crates_core_model_execution_mode) | required |
| `unit` | string or null | optional |

<a id="crates_core_model_execution_mode"></a>

### Mode

`crates_core_model_execution_Mode`

`"process"`

`"systemd"`

<a id="crates_core_model_health_observation"></a>

### Observation

`crates_core_model_health_Observation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `status` | integer | required |
| `body_sha256` | string | required |
| `bytes` | integer | required |

<a id="crates_core_model_host_reboot"></a>

### Reboot

`crates_core_model_host_Reboot`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `before_boot` | string | required |
| `after_boot` | string or null | optional |
| `kernel` | string or null | optional |
| `requested_ms` | integer | required |

<a id="crates_core_model_logs_logs"></a>

### Logs

`crates_core_model_logs_Logs`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `stdout` | [Stream](api-types.md#crates_core_model_logs_stream) | required |
| `stderr` | [Stream](api-types.md#crates_core_model_logs_stream) | required |

<a id="crates_core_model_logs_stream"></a>

### Stream

`crates_core_model_logs_Stream`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `stored_bytes` | integer | required |
| `seen_bytes` | integer | required |
| `sha256` | string | required |
| `tail` | string | required |
| `truncated` | boolean | required |

<a id="crates_core_model_outputs_materialized"></a>

### Materialized

`crates_core_model_outputs_Materialized`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `work` | [Work](api-types.md#crates_core_model_domain_work) | required |
| `input_sha256` | string | required |
| `source_sha256` | map<string, string> | required |

Additional fields are rejected.

<a id="crates_core_model_outputs_publickey"></a>

### PublicKey

`crates_core_model_outputs_PublicKey`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `id` | string | required |
| `public_key` | string | required |

Additional fields are rejected.

<a id="crates_core_model_outputs_reference"></a>

### Reference

`crates_core_model_outputs_Reference`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `step` | string | required |
| `name` | string | required |
| `node` | string or null | optional |
| `batch` | integer or null | optional |
| `pointer` | string or null | optional |
| `collect` | boolean | optional |

Additional fields are rejected.

<a id="crates_core_model_outputs_sealedvalue"></a>

### SealedValue

`crates_core_model_outputs_SealedValue`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `key_id` | string | required |
| `ephemeral_key` | string | required |
| `nonce` | string | required |
| `ciphertext` | string | required |
| `plaintext_bytes` | integer | required |

Additional fields are rejected.

<a id="crates_core_model_outputs_specification"></a>

### Specification

`crates_core_model_outputs_Specification`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `type` | [ValueType](api-types.md#crates_core_model_outputs_valuetype) | required |
| `secret` | boolean | optional |
| `max_bytes` | integer | optional |
| `ttl_seconds` | integer | optional |
| `source` | string or null | optional |

Additional fields are rejected.

<a id="crates_core_model_outputs_storedvalue"></a>

### StoredValue

`crates_core_model_outputs_StoredValue`

Discriminator `kind=public`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `value` | any JSON | required |
| `kind` | `"public"` | required |

Additional fields are rejected.

Discriminator `kind=secret`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sealed` | [SealedValue](api-types.md#crates_core_model_outputs_sealedvalue) | required |
| `kind` | `"secret"` | required |

Additional fields are rejected.

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

| Field | JSON type | Required / default |
| --- | --- | --- |
| `before` | string or null | optional |
| `after` | string or null | optional |
| `transaction_exit` | integer or null | optional |
| `diagnostic` | string | required |

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

| Field | JSON type | Required / default |
| --- | --- | --- |
| `outputs` | map<string, [StoredValue](api-types.md#crates_core_model_outputs_storedvalue)> | optional |
| `diagnostic` | string or null | optional |
| `execution` | [Evidence](api-types.md#crates_core_model_execution_evidence) or null | optional |
| `logs` | [Logs](api-types.md#crates_core_model_logs_logs) or null | optional |
| `native` | [Evidence](api-types.md#crates_core_model_evidence_evidence) or null | optional |
| `operation_id` | string | required |
| `script_sha256` | string | required |
| `timeout_seconds` | integer | required |
| `outcome` | [Outcome](api-types.md#crates_core_model_receipt_outcome) | required |
| `exit_code` | integer or null | optional |
| `boot_id` | string | required |

<a id="crates_core_model_remote_logs_manifest"></a>

### Manifest

`crates_core_model_remote_logs_Manifest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `chunk_hashes` | string[] | optional |
| `missing_ranges` | [integer, integer][] | optional |
| `run_id` | string | required |
| `work_id` | string | required |
| `node_uid` | string | required |
| `grant` | string | required |
| `store_id` | string | required |
| `stream` | string | required |
| `bytes` | integer | required |
| `sha256` | string | required |
| `chunks` | integer | required |
| `truncated` | boolean | required |
| `expires_ms` | integer | required |

Additional fields are rejected.

<a id="crates_core_model_service_state"></a>

### State

`crates_core_model_service_State`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `executable_sha256` | string or null | optional |
| `main_pid` | integer or null | optional |
| `load` | string | required |
| `active` | string | required |
| `enabled` | string | required |
| `sub` | string | required |

<a id="crates_core_model_target_externaltarget"></a>

### ExternalTarget

`crates_core_model_target_ExternalTarget`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `revision` | integer | required |
| `project` | string | optional; default `"default"` |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | required |
| `connection` | string | required |
| `spec` | any JSON | required |

Additional fields are rejected.

<a id="crates_core_model_telemetry_snapshot"></a>

### Snapshot

`crates_core_model_telemetry_Snapshot`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `collected_ms` | integer | required |
| `data` | map<string, any JSON> | required |
| `errors` | map<string, string> | optional |
| `truncated` | boolean | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_administration_administration"></a>

### Administration

`crates_core_orchestration_administration_Administration`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `secrets` | map<string, map<string, [SealedValue](api-types.md#crates_core_model_outputs_sealedvalue)>> | optional |
| `artifacts` | map<string, integer> | optional |
| `retired_node_groups` | map<string, [NodeGroup](api-types.md#crates_core_model_domain_nodegroup)> | optional |
| `windows` | map<string, [MaintenanceWindow](api-types.md#crates_core_orchestration_administration_maintenancewindow)> | optional |
| `owner_revisions` | map<string, integer> | optional |
| `maintenance` | string[] | required |
| `audit` | [Change](api-types.md#crates_core_orchestration_administration_change)[] | required |

<a id="crates_core_orchestration_administration_change"></a>

### Change

`crates_core_orchestration_administration_Change`

Discriminator `action=secret`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `name` | string | required |
| `version` | string | required |
| `sealed` | [SealedValue](api-types.md#crates_core_model_outputs_sealedvalue) | required |
| `action` | `"secret"` | required |

Additional fields are rejected.

Discriminator `action=artifact`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sha256` | string | required |
| `size` | integer | required |
| `action` | `"artifact"` | required |

Additional fields are rejected.

Discriminator `action=log_pruned`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `key` | string | required |
| `store_id` | string | required |
| `sha256` | string | required |
| `action` | `"log_pruned"` | required |

Additional fields are rejected.

Discriminator `action=log_storage`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `store_id` | string | required |
| `action` | `"log_storage"` | required |

Additional fields are rejected.

Discriminator `action=log_final`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `manifest` | [Manifest](api-types.md#crates_core_model_remote_logs_manifest) | required |
| `action` | `"log_final"` | required |

Additional fields are rejected.

Discriminator `action=fence_floor`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `expected_sequence` | integer | required |
| `floor` | integer | required |
| `evidence` | string | required |
| `action` | `"fence_floor"` | required |

Additional fields are rejected.

Discriminator `action=target`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `target` | [ExternalTarget](api-types.md#crates_core_model_target_externaltarget) | required |
| `action` | `"target"` | required |

Additional fields are rejected.

Discriminator `action=delete_node_group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `expected_revision` | integer | required |
| `reason` | string | required |
| `action` | `"delete_node_group"` | required |

Additional fields are rejected.

Discriminator `action=maintenance_window`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `window` | [MaintenanceWindow](api-types.md#crates_core_orchestration_administration_maintenancewindow) | required |
| `expected_revision` | integer | required |
| `evidence` | string | required |
| `action` | `"maintenance_window"` | required |

Additional fields are rejected.

Discriminator `action=create_node_group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `group` | [NodeGroup](api-types.md#crates_core_model_domain_nodegroup) | required |
| `expected_revision` | integer | required |
| `action` | `"create_node_group"` | required |

Additional fields are rejected.

Discriminator `action=update_node_group`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `group` | [NodeGroup](api-types.md#crates_core_model_domain_nodegroup) | required |
| `expected_revision` | integer | required |
| `action` | `"update_node_group"` | required |

Additional fields are rejected.

Discriminator `action=maintenance`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_uid` | string | required |
| `enabled` | boolean | required |
| `action` | `"maintenance"` | required |

Additional fields are rejected.

Discriminator `action=ownership`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `resource` | string | required |
| `expected_owner` | string | required |
| `expected_revision` | integer | required |
| `new_owner` | string or null | optional |
| `evidence` | string | required |
| `action` | `"ownership"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_administration_maintenancewindow"></a>

### MaintenanceWindow

`crates_core_orchestration_administration_MaintenanceWindow`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `revision` | integer | required |
| `node_uids` | string[] | required |
| `start_ms` | integer | required |
| `end_ms` | integer | required |
| `enabled` | boolean | required |

Additional fields are rejected.

<a id="crates_core_orchestration_controlplane_catalog"></a>

### Catalog

`crates_core_orchestration_controlplane_Catalog`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `active` | string or null | optional |
| `operations` | map<string, [Operation](api-types.md#crates_core_orchestration_controlplane_operation)> | required |

<a id="crates_core_orchestration_controlplane_change"></a>

### Change

`crates_core_orchestration_controlplane_Change`

Discriminator `action=request`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `intent` | [Replace](api-types.md#crates_core_orchestration_controlplane_replace) | required |
| `existing_fingerprints` | string[] | required |
| `action` | `"request"` | required |

Additional fields are rejected.

Discriminator `action=observe`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `expected_revision` | integer | required |
| `condition` | string | required |
| `diagnostic` | string or null | optional |
| `action` | `"observe"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_controlplane_operation"></a>

### Operation

`crates_core_orchestration_controlplane_Operation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `intent` | [Replace](api-types.md#crates_core_orchestration_controlplane_replace) | required |
| `condition` | string | required |
| `diagnostic` | string or null | optional |
| `revision` | integer | required |

<a id="crates_core_orchestration_controlplane_replace"></a>

### Replace

`crates_core_orchestration_controlplane_Replace`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `old_id` | integer | required |
| `new_id` | integer | required |
| `address` | string | required |
| `old_fingerprint` | string | required |
| `new_fingerprint` | string | required |
| `expected_membership` | string | required |
| `voters` | integer[] | required |
| `evidence` | string | required |

Additional fields are rejected.

<a id="crates_core_orchestration_drift_policy"></a>

### Policy

`crates_core_orchestration_drift_Policy`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `configuration_revision` | integer | optional |
| `configuration_evidence` | string or null | optional |
| `interval_seconds` | integer or null | optional |
| `last_started_ms` | integer | required |
| `sequence` | integer | required |
| `active_run` | string or null | optional |
| `last_run` | string or null | optional |
| `target_generation` | integer | required |
| `observed_generation` | integer or null | optional |
| `condition` | string | required |

<a id="crates_core_orchestration_history_change"></a>

### Change

`crates_core_orchestration_history_Change`

Discriminator `action=clear_runs`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `expected_sequence` | integer | required |
| `backup_sha256` | string | required |
| `evidence` | string | required |
| `action` | `"clear_runs"` | required |

Additional fields are rejected.

Discriminator `action=archive`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kind` | [Kind](api-types.md#crates_core_orchestration_history_kind) | required |
| `uid` | string | required |
| `expected_revision` | integer | required |
| `now_ms` | integer | required |
| `action` | `"archive"` | required |

Additional fields are rejected.

Discriminator `action=configure`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `expected_revision` | integer | required |
| `keep_runs` | integer | required |
| `definition_hot_bytes` | integer | required |
| `evidence` | string | required |
| `action` | `"configure"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_history_history"></a>

### History

`crates_core_orchestration_history_History`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `records` | map<string, [Record](api-types.md#crates_core_orchestration_history_record)> | required |
| `keep_runs` | integer | required |
| `definition_hot_bytes` | integer | required |
| `configuration_revision` | integer | required |
| `audit` | any JSON[] | required |

<a id="crates_core_orchestration_history_kind"></a>

### Kind

`crates_core_orchestration_history_Kind`

`"run"`

`"definition"`

<a id="crates_core_orchestration_history_record"></a>

### Record

`crates_core_orchestration_history_Record`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `submitted_ms` | integer or null | optional |
| `finished_ms` | integer or null | optional |
| `kind` | [Kind](api-types.md#crates_core_orchestration_history_kind) | required |
| `uid` | string | required |
| `sha256` | string | required |
| `bytes` | integer | required |
| `revision` | integer | required |
| `owner` | string | required |
| `project` | string or null | optional |
| `node_group_uid` | string or null | optional |
| `target` | string | required |
| `sequence` | integer | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) or null | optional |
| `plan_sha256` | string | required |
| `total` | integer | required |
| `completed` | integer | required |
| `archived_ms` | integer | required |

<a id="crates_core_orchestration_inventory_host"></a>

### Host

`crates_core_orchestration_inventory_Host`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `address` | string | required |
| `hostname` | string | required |
| `values` | map<string, any JSON> | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_inventory_inventory"></a>

### Inventory

`crates_core_orchestration_inventory_Inventory`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `nodes` | [Host](api-types.md#crates_core_orchestration_inventory_host)[] | required |
| `groups` | map<string, string[]> | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_inventory_packagefile"></a>

### PackageFile

`crates_core_orchestration_inventory_PackageFile`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sha256` | string | required |
| `executable` | boolean | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_machine_types_attempt"></a>

### Attempt

`crates_core_orchestration_machine_types_Attempt`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `completed_ms` | integer or null | optional |
| `materialized` | [Materialized](api-types.md#crates_core_model_outputs_materialized) or null | optional |
| `authorized_ms` | integer or null | optional |
| `reason` | string or null | optional |
| `receipt` | [Receipt](api-types.md#crates_core_model_receipt_receipt) or null | optional |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `grant` | string or null | optional |
| `boot_id` | string or null | optional |
| `session_id` | string or null | optional |
| `last_sequence` | integer | required |
| `reports` | map<string, string> | required |

<a id="crates_core_orchestration_machine_types_audithead"></a>

### AuditHead

`crates_core_orchestration_machine_types_AuditHead`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `count` | integer | required |
| `sha256` | string | required |

<a id="crates_core_orchestration_machine_types_command"></a>

### Command

`crates_core_orchestration_machine_types_Command`

Discriminator `type=access`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_access_change) | required |
| `type` | `"access"` | required |

Additional fields are rejected.

Discriminator `type=retry_inputs`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `expected_revision` | integer | required |
| `type` | `"retry_inputs"` | required |

Additional fields are rejected.

Discriminator `type=materialize`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `now_ms` | integer | required |
| `type` | `"materialize"` | required |

Additional fields are rejected.

Discriminator `type=workflow`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_workflow_change) | required |
| `type` | `"workflow"` | required |

Additional fields are rejected.

Discriminator `type=personnel`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_personnel_change) | required |
| `type` | `"personnel"` | required |

Additional fields are rejected.

Discriminator `type=prepared`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `command` | [Command](api-types.md#crates_core_orchestration_machine_types_command) | required |
| `contracts` | any JSON | required |
| `type` | `"prepared"` | required |

Additional fields are rejected.

Discriminator `type=history`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_history_change) | required |
| `type` | `"history"` | required |

Additional fields are rejected.

Discriminator `type=controlplane`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_controlplane_change) | required |
| `type` | `"controlplane"` | required |

Additional fields are rejected.

Discriminator `type=preparation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_preparation_change) | required |
| `type` | `"preparation"` | required |

Additional fields are rejected.

Discriminator `type=pause_run`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `paused` | boolean | required |
| `expected_revision` | integer | required |
| `type` | `"pause_run"` | required |

Additional fields are rejected.

Discriminator `type=authorized`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `authorization` | [Authorization](api-types.md#crates_core_identities_registration_authorization) | required |
| `command` | [Command](api-types.md#crates_core_orchestration_machine_types_command) | required |
| `type` | `"authorized"` | required |

Additional fields are rejected.

Discriminator `type=identity`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_registration_change) | required |
| `type` | `"identity"` | required |

Additional fields are rejected.

Discriminator `type=configure_drift`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `interval_seconds` | integer or null | optional |
| `type` | `"configure_drift"` | required |

Additional fields are rejected.

Discriminator `type=reconcile_drift`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `now_ms` | integer | required |
| `expected_generation` | integer | required |
| `type` | `"reconcile_drift"` | required |

Additional fields are rejected.

Discriminator `type=administer`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_administration_change) | required |
| `type` | `"administer"` | required |

Additional fields are rejected.

Discriminator `type=set_certificate_binding`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_identities_identity_change) | required |
| `type` | `"set_certificate_binding"` | required |

Additional fields are rejected.

Discriminator `type=apply_deployment`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | required |
| `type` | `"apply_deployment"` | required |

Additional fields are rejected.

Discriminator `type=reconcile_deployment`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `type` | `"reconcile_deployment"` | required |

Additional fields are rejected.

Discriminator `type=observe_node`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `diagnostic` | string or null | optional |
| `inventory` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) or null | optional |
| `resources` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) or null | optional |
| `inventory_checked_ms` | integer or null | optional |
| `capabilities` | [Identity](api-types.md#crates_plugin_api_lib_identity)[] | optional |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) or null | optional |
| `hardware_sha256` | string or null | optional |
| `node_uid` | string | required |
| `boot_id` | string | required |
| `session_id` | string | required |
| `observed_ms` | integer | required |
| `type` | `"observe_node"` | required |

Additional fields are rejected.

Discriminator `type=put_script`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sha256` | string | required |
| `content` | string | required |
| `type` | `"put_script"` | required |

Additional fields are rejected.

Discriminator `type=create_run`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) or null | optional |
| `run_id` | string | required |
| `idempotency_key` | string | required |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | required |
| `type` | `"create_run"` | required |

Additional fields are rejected.

Discriminator `type=claim`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `input_sha256` | string or null | optional |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) or null | optional |
| `authorized_ms` | integer or null | optional |
| `run_id` | string | required |
| `work_id` | string | required |
| `node_uid` | string | required |
| `grant` | string | required |
| `boot_id` | string | required |
| `session_id` | string | required |
| `type` | `"claim"` | required |

Additional fields are rejected.

Discriminator `type=report`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) or null | optional |
| `run_id` | string | required |
| `work_id` | string | required |
| `node_uid` | string | required |
| `grant` | string | required |
| `sequence` | integer | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `receipt` | [Receipt](api-types.md#crates_core_model_receipt_receipt) or null | optional |
| `type` | `"report"` | required |

Additional fields are rejected.

Discriminator `type=cancel`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `type` | `"cancel"` | required |

Additional fields are rejected.

Discriminator `type=mark_uncertain`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `reason` | string or null | optional |
| `run_id` | string | required |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `type` | `"mark_uncertain"` | required |

Additional fields are rejected.

Discriminator `type=resolve`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `actor` | string | required |
| `evidence` | string | required |
| `type` | `"resolve"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_machine_types_deployment"></a>

### Deployment

`crates_core_orchestration_machine_types_Deployment`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `generation` | integer | required |
| `applied_generation` | integer or null | optional |
| `last_started_generation` | integer | required |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | required |
| `active_run` | string or null | optional |
| `last_run` | string or null | optional |
| `condition` | string | required |

<a id="crates_core_orchestration_machine_types_nodeobservation"></a>

### NodeObservation

`crates_core_orchestration_machine_types_NodeObservation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `diagnostic` | string or null | optional |
| `inventory` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) or null | optional |
| `resources` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) or null | optional |
| `inventory_checked_ms` | integer or null | optional |
| `capabilities` | [Identity](api-types.md#crates_plugin_api_lib_identity)[] | optional |
| `hardware_sha256` | string or null | optional |
| `boot_id` | string | required |
| `session_id` | string | required |
| `observed_ms` | integer | required |

<a id="crates_core_orchestration_machine_types_reply"></a>

### Reply

`crates_core_orchestration_machine_types_Reply`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

<a id="crates_core_orchestration_machine_types_resolution"></a>

### Resolution

`crates_core_orchestration_machine_types_Resolution`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `work_id` | string | required |
| `actor` | string | required |
| `evidence` | string | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |

<a id="crates_core_orchestration_machine_types_run"></a>

### Run

`crates_core_orchestration_machine_types_Run`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) or null | optional |
| `submitted_ms` | integer or null | optional |
| `finished_ms` | integer or null | optional |
| `artifact_nodes` | string[] or null | optional |
| `preparing` | boolean | optional |
| `prepared` | string[] | optional |
| `preparation_errors` | map<string, string> | optional |
| `sequence` | integer | optional |
| `pause_requested` | boolean | optional |
| `id` | string | required |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `cancel_requested` | boolean | required |
| `revision` | integer | required |
| `attempts` | map<string, [Attempt](api-types.md#crates_core_orchestration_machine_types_attempt)> | required |

<a id="crates_core_orchestration_machine_types_state"></a>

### State

`crates_core_orchestration_machine_types_State`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflows` | [Registry](api-types.md#crates_core_orchestration_workflow_registry) | optional |
| `personnel` | [Registry](api-types.md#crates_core_identities_personnel_registry) | optional |
| `log_pruned` | map<string, string> | optional |
| `log_store_id` | string or null | optional |
| `log_manifests` | map<string, [Manifest](api-types.md#crates_core_model_remote_logs_manifest)> | optional |
| `cold_audit` | map<string, [AuditHead](api-types.md#crates_core_orchestration_machine_types_audithead)> | optional |
| `retirement_proofs` | map<string, string> | optional |
| `targets` | map<string, [ExternalTarget](api-types.md#crates_core_model_target_externaltarget)> | optional |
| `history` | [History](api-types.md#crates_core_orchestration_history_history) | optional |
| `controlplane` | [Catalog](api-types.md#crates_core_orchestration_controlplane_catalog) | optional |
| `run_sequence` | integer | optional |
| `registry` | [Registry](api-types.md#crates_core_identities_registration_registry) | optional |
| `drift` | map<string, [Policy](api-types.md#crates_core_orchestration_drift_policy)> | optional |
| `owner_projects` | map<string, string or null> | optional |
| `administration` | [Administration](api-types.md#crates_core_orchestration_administration_administration) | optional |
| `identities` | [Identities](api-types.md#crates_core_identities_identity_identities) | optional |
| `deployments` | map<string, [Deployment](api-types.md#crates_core_orchestration_machine_types_deployment)> | optional |
| `observations` | map<string, [NodeObservation](api-types.md#crates_core_orchestration_machine_types_nodeobservation)> | optional |
| `scripts` | map<string, string> | optional |
| `node_groups` | map<string, [NodeGroup](api-types.md#crates_core_model_domain_nodegroup)> | required |
| `runs` | map<string, [Run](api-types.md#crates_core_orchestration_machine_types_run)> | required |
| `execution_locks` | map<string, string> | required |
| `resource_owners` | map<string, string> | required |
| `idempotency` | map<string, [string, string]> | required |
| `audit` | [Resolution](api-types.md#crates_core_orchestration_machine_types_resolution)[] | required |

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

Discriminator `action=prepared`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `work_id` | string | required |
| `node_uid` | string | required |
| `sha256` | string | required |
| `size` | integer | required |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) or null | optional |
| `action` | `"prepared"` | required |

Additional fields are rejected.

Discriminator `action=failed`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `work_id` | string | required |
| `node_uid` | string | required |
| `reason` | string | required |
| `proof` | [Proof](api-types.md#crates_core_identities_registration_proof) or null | optional |
| `action` | `"failed"` | required |

Additional fields are rejected.

Discriminator `action=admit`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `expected_revision` | integer | required |
| `action` | `"admit"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_actor"></a>

### Actor

`crates_core_orchestration_workflow_Actor`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `name` | string | required |
| `fingerprint` | string | required |
| `credential_uid` | string | required |

<a id="crates_core_orchestration_workflow_binding"></a>

### Binding

`crates_core_orchestration_workflow_Binding`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `step_id` | string | required |
| `path` | string | required |
| `transform` | [BindingTransform](api-types.md#crates_core_orchestration_workflow_bindingtransform) or null | optional |
| `allowed_json_paths` | string[] | optional |
| `scalar_rules` | map<string, [ScalarRule](api-types.md#crates_core_orchestration_workflow_scalarrule)> | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_bindingtransform"></a>

### BindingTransform

`crates_core_orchestration_workflow_BindingTransform`

`"sha256"`

<a id="crates_core_orchestration_workflow_change"></a>

### Change

`crates_core_orchestration_workflow_Change`

Discriminator `action=register`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflow` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | required |
| `expected_revision` | integer | required |
| `action` | `"register"` | required |

Additional fields are rejected.

Discriminator `action=enable`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `enabled` | boolean | required |
| `expected_revision` | integer | required |
| `action` | `"enable"` | required |

Additional fields are rejected.

Discriminator `action=execute`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) or null | optional |
| `request` | [Request](api-types.md#crates_core_orchestration_workflow_request) | required |
| `actor` | [Actor](api-types.md#crates_core_orchestration_workflow_actor) | required |
| `now_ms` | integer | required |
| `action` | `"execute"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_execution"></a>

### Execution

`crates_core_orchestration_workflow_Execution`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request` | [Request](api-types.md#crates_core_orchestration_workflow_request) | required |
| `actor` | [Actor](api-types.md#crates_core_orchestration_workflow_actor) | required |
| `parameters` | map<string, any JSON> | required |
| `submitted_ms` | integer | required |
| `deployment_uid` | string or null | optional |
| `system` | boolean | optional |

<a id="crates_core_orchestration_workflow_parameter"></a>

### Parameter

`crates_core_orchestration_workflow_Parameter`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kind` | [ParameterType](api-types.md#crates_core_orchestration_workflow_parametertype) | required |
| `default` | any JSON or null | optional |
| `choices` | any JSON[] | optional |
| `minimum` | number or null | optional |
| `maximum` | number or null | optional |
| `max_length` | integer or null | optional |
| `bindings` | [Binding](api-types.md#crates_core_orchestration_workflow_binding)[] | required |

Additional fields are rejected.

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

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflows` | map<string, [Workflow](api-types.md#crates_core_orchestration_workflow_workflow)> | required |
| `revisions` | map<string, [Revision](api-types.md#crates_core_orchestration_workflow_revision)> | required |
| `executions` | map<string, [Execution](api-types.md#crates_core_orchestration_workflow_execution)> | required |

<a id="crates_core_orchestration_workflow_request"></a>

### Request

`crates_core_orchestration_workflow_Request`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflow_uid` | string | required |
| `workflow_revision` | string | required |
| `target` | string | required |
| `parameters` | map<string, any JSON> | optional |
| `request_id` | string | required |
| `plan_sha256` | string | required |
| `target_sha256` | string | required |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_revision"></a>

### Revision

`crates_core_orchestration_workflow_Revision`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `inventory` | [Inventory](api-types.md#crates_core_orchestration_inventory_inventory) or null | optional |
| `bundle` | map<string, [PackageFile](api-types.md#crates_core_orchestration_inventory_packagefile)> | optional |
| `uid` | string | required |
| `name` | string | required |
| `playbook` | [Playbook](api-types.md#crates_core_model_domain_playbook) | required |
| `targets` | map<string, [TargetPreset](api-types.md#crates_core_orchestration_workflow_targetpreset)> | required |
| `fixed` | map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | optional |
| `values` | map<string, any JSON> | optional |
| `immutable` | string[] | optional |
| `node_values` | map<string, map<string, any JSON>> | optional |
| `parameters` | map<string, [Parameter](api-types.md#crates_core_orchestration_workflow_parameter)> | optional |
| `definitions` | map<string, string> | optional |
| `provenance` | map<string, string> | optional |
| `batch_size` | integer | optional |
| `download_parallel` | integer | optional |
| `reconcile_seconds` | integer or null | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_scalarrule"></a>

### ScalarRule

`crates_core_orchestration_workflow_ScalarRule`

Discriminator `kind=integer`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `minimum` | integer | required |
| `maximum` | integer | required |
| `kind` | `"integer"` | required |

Additional fields are rejected.

Discriminator `kind=quantity`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kind` | `"quantity"` | required |

Additional fields are rejected.

Discriminator `kind=duration`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kind` | `"duration"` | required |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_targetpreset"></a>

### TargetPreset

`crates_core_orchestration_workflow_TargetPreset`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `hosts` | string or null | optional |
| `owner_uid` | string or null | optional |
| `node_group_uid` | string or null | optional |
| `external_target_uid` | string or null | optional |

Additional fields are rejected.

<a id="crates_core_orchestration_workflow_workflow"></a>

### Workflow

`crates_core_orchestration_workflow_Workflow`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `current` | string | required |
| `revision` | integer | required |
| `enabled` | boolean | required |

<a id="crates_plugin_api_lib_call"></a>

### Call

`crates_plugin_api_lib_Call`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | required |
| `action` | string | required |
| `location` | [Location](api-types.md#crates_plugin_api_lib_location) | required |
| `spec` | any JSON | required |
| `resources` | string[] | required |
| `reconcilable` | boolean | required |
| `timeout_seconds` | integer | required |

Additional fields are rejected.

<a id="crates_plugin_api_lib_descriptor"></a>

### Descriptor

`crates_plugin_api_lib_Descriptor`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#crates_plugin_api_lib_identity) | required |
| `location` | [Location](api-types.md#crates_plugin_api_lib_location) | required |
| `actions` | string[] | required |
| `description` | string | required |

<a id="crates_plugin_api_lib_expansion"></a>

### Expansion

`crates_plugin_api_lib_Expansion`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | required |
| `action` | string | required |
| `steps` | any JSON[] | required |
| `definitions` | map<string, string> | required |

<a id="crates_plugin_api_lib_identity"></a>

### Identity

`crates_plugin_api_lib_Identity`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `id` | string | required |
| `version` | string | required |
| `contract` | integer | required |
| `sha256` | string | required |

Additional fields are rejected.

<a id="crates_plugin_api_lib_location"></a>

### Location

`crates_plugin_api_lib_Location`

`"node"`

`"controller"`

<a id="crates_plugin_api_lib_observation"></a>

### Observation

`crates_plugin_api_lib_Observation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `owner` | string | required |
| `fence` | integer | required |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | required |
| `call_sha256` | string | required |
| `payload` | any JSON | required |
| `verified` | boolean | required |

Additional fields are rejected.

<a id="crates_plugin_api_lib_scriptorigin"></a>

### ScriptOrigin

`crates_plugin_api_lib_ScriptOrigin`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `plugin` | [Identity](api-types.md#crates_plugin_api_lib_identity) | required |
| `action` | string | required |

Additional fields are rejected.

<a id="crates_plugin_api_variables_outputsecretreference"></a>

### OutputSecretReference

`crates_plugin_api_variables_OutputSecretReference`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `output_secret` | [OutputSecretSource](api-types.md#crates_plugin_api_variables_outputsecretsource) | required |

Additional fields are rejected.

<a id="crates_plugin_api_variables_outputsecretsource"></a>

### OutputSecretSource

`crates_plugin_api_variables_OutputSecretSource`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `run_id` | string | required |
| `step` | string | required |
| `name` | string | required |
| `work_ids` | string[] | required |
| `pointer` | string or null | optional |
| `collect` | boolean | required |
| `sha256` | string | required |

Additional fields are rejected.

<a id="crates_plugin_api_variables_reference"></a>

### Reference

`crates_plugin_api_variables_Reference`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `secret` | string | required |
| `version` | string | required |

Additional fields are rejected.

<a id="crates_plugin_api_variables_resolved"></a>

### Resolved

`crates_plugin_api_variables_Resolved`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `values` | map<string, string> | required |
| `redactions` | integer[][] | required |

<a id="crates_plugin_api_variables_value"></a>

### Value

`crates_plugin_api_variables_Value`

string

[Reference](api-types.md#crates_plugin_api_variables_reference)

| Field | JSON type | Required / default |
| --- | --- | --- |
| `secret` | string | required |
| `version` | string | required |

Additional fields are rejected.

[OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)

| Field | JSON type | Required / default |
| --- | --- | --- |
| `output_secret` | [OutputSecretSource](api-types.md#crates_plugin_api_variables_outputsecretsource) | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_access_accesscheck"></a>

### AccessCheck

`crates_runtime_controller_server_access_AccessCheck`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `action` | [Action](api-types.md#crates_core_identities_access_action) | required |
| `workflow` | string or null | optional |
| `preset` | string or null | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_artifacts_artifactupload"></a>

### ArtifactUpload

`crates_runtime_controller_server_artifacts_ArtifactUpload`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `size` | integer | required |
| `offset` | integer | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_identity_activaterequest"></a>

### ActivateRequest

`crates_runtime_controller_server_identity_ActivateRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_identity_bootstraprequest"></a>

### BootstrapRequest

`crates_runtime_controller_server_identity_BootstrapRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `id` | string | required |
| `node_uid` | string | required |
| `token_sha256` | string | required |
| `csr_pem` | string | required |
| `ttl_seconds` | integer | required |
| `recover` | boolean | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_identity_enrollrequest"></a>

### EnrollRequest

`crates_runtime_controller_server_identity_EnrollRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string | required |
| `grant_id` | string | required |
| `node_uid` | string | required |
| `hardware_sha256` | string | required |
| `token_sha256` | string | required |
| `csr_pem` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_identity_renewalrequest"></a>

### RenewalRequest

`crates_runtime_controller_server_identity_RenewalRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string | required |
| `csr_pem` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_identity_retirerequest"></a>

### RetireRequest

`crates_runtime_controller_server_identity_RetireRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_uid` | string | required |
| `expected_fingerprint` | string | required |
| `evidence` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_outputs_inputsquery"></a>

### InputsQuery

`crates_runtime_controller_server_outputs_InputsQuery`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_product_auditquery"></a>

### AuditQuery

`crates_runtime_controller_server_product_AuditQuery`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `after` | integer | optional |
| `limit` | integer | optional; default `50` |
| `stream` | string | optional; default `"commands"` |
| `view` | string | optional; default `"all"` |

Additional fields are rejected.

<a id="crates_runtime_controller_server_queries_pagequery"></a>

### PageQuery

`crates_runtime_controller_server_queries_PageQuery`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `search` | string or null | optional |
| `workflow` | string or null | optional |
| `preset` | string or null | optional |
| `actor` | string or null | optional |
| `after` | string or null | optional |
| `limit` | integer | optional; default `50` |
| `status` | string or null | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_remote_logs_chunk"></a>

### Chunk

`crates_runtime_controller_server_remote_logs_Chunk`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant` | string or null | optional |
| `offset` | integer | required |
| `data` | string | required |
| `sha256` | string | required |
| `complete` | boolean | optional |
| `missing` | boolean | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_remote_logs_logquery"></a>

### LogQuery

`crates_runtime_controller_server_remote_logs_LogQuery`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `offset` | integer | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_runs_heartbeat"></a>

### Heartbeat

`crates_runtime_controller_server_runs_Heartbeat`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `diagnostic` | string or null | optional |
| `inventory` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) or null | optional |
| `resources` | [Snapshot](api-types.md#crates_core_model_telemetry_snapshot) or null | optional |
| `inventory_checked_ms` | integer or null | optional |
| `capabilities` | [Identity](api-types.md#crates_plugin_api_lib_identity)[] | optional |
| `hardware_sha256` | string or null | optional |
| `boot_id` | string | required |
| `session_id` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_runs_pauserequest"></a>

### PauseRequest

`crates_runtime_controller_server_runs_PauseRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `paused` | boolean | required |
| `expected_revision` | integer | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_runs_poll"></a>

### Poll

`crates_runtime_controller_server_runs_Poll`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `after` | integer or null | optional |
| `wait_ms` | integer | optional |

Additional fields are rejected.

<a id="crates_runtime_controller_server_runs_resolverequest"></a>

### ResolveRequest

`crates_runtime_controller_server_runs_ResolveRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `actor` | string | required |
| `evidence` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_runs_uncertainrequest"></a>

### UncertainRequest

`crates_runtime_controller_server_runs_UncertainRequest`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `reason` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_secrets_registersecret"></a>

### RegisterSecret

`crates_runtime_controller_server_secrets_RegisterSecret`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `name` | string | required |
| `version` | string | required |
| `value` | string | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_workflows_enableworkflow"></a>

### EnableWorkflow

`crates_runtime_controller_server_workflows_EnableWorkflow`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `enabled` | boolean | required |
| `expected_revision` | integer | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_workflows_registerworkflow"></a>

### RegisterWorkflow

`crates_runtime_controller_server_workflows_RegisterWorkflow`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflow` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | required |
| `expected_revision` | integer | required |

Additional fields are rejected.

<a id="crates_runtime_controller_server_workflows_workflowpreview"></a>

### WorkflowPreview

`crates_runtime_controller_server_workflows_WorkflowPreview`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflow_revision` | string | required |
| `target` | string | required |
| `parameters` | map<string, any JSON> | optional |
| `request_id` | string | required |

Additional fields are rejected.

<a id="crates_runtime_storage_store_machine"></a>

### Machine

`crates_runtime_storage_store_Machine`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `last_applied` | [RaftLogId](api-types.md#raftlogid) or null | optional |
| `membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | required |
| `state` | [State](api-types.md#crates_core_orchestration_machine_types_state) | required |

<a id="crates_runtime_storage_store_position"></a>

### Position

`crates_runtime_storage_store_Position`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `last_applied` | [RaftLogId](api-types.md#raftlogid) or null | optional |
| `membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | required |

<a id="plugins_docker_container_credential"></a>

### Credential

`plugins_docker_container_Credential`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `registry` | string | required |
| `directory` | string | required |
| `projects` | string[] | optional |

Additional fields are rejected.

<a id="plugins_docker_container_mount"></a>

### Mount

`plugins_docker_container_Mount`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `source` | string | required |
| `target` | string | required |
| `read_only` | boolean | required |

Additional fields are rejected.

<a id="plugins_docker_container_observation"></a>

### Observation

`plugins_docker_container_Observation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `container_id` | string | required |
| `image_id` | string | required |
| `image_digest` | string | required |
| `platform` | string | required |
| `owner` | string | required |
| `configuration_sha256` | string | required |
| `healthy` | boolean | required |

<a id="plugins_docker_container_port"></a>

### Port

`plugins_docker_container_Port`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `host` | integer | required |
| `container` | integer | required |
| `address` | string | optional; default `"127.0.0.1"` |

Additional fields are rejected.

<a id="plugins_docker_container_rules"></a>

### Rules

`plugins_docker_container_Rules`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `names` | string[] | required |
| `registries` | string[] | required |
| `volume_roots` | string[] | optional |
| `credentials` | map<string, [Credential](api-types.md#plugins_docker_container_credential)> | optional |
| `allow_root_user` | boolean | optional |

Additional fields are rejected.

<a id="plugins_docker_container_spec"></a>

### Spec

`plugins_docker_container_Spec`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `name` | string | required |
| `image` | string | required |
| `platform` | string | required |
| `command` | string[] | required |
| `environment` | map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | optional |
| `ports` | [Port](api-types.md#plugins_docker_container_port)[] | optional |
| `mounts` | [Mount](api-types.md#plugins_docker_container_mount)[] | optional |
| `healthcheck` | string | required |
| `readiness_timeout_seconds` | integer | required |
| `user_id` | integer | optional; default `65534` |
| `memory_bytes` | integer | optional; default `268435456` |
| `cpu_millis` | integer | optional; default `1000` |
| `credential_ref` | [Reference](api-types.md#crates_plugin_api_variables_reference) or null | optional |

Additional fields are rejected.

<a id="plugins_kubeadm_lib_input"></a>

### Input

`plugins_kubeadm_lib_Input`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `version` | string | required |
| `environment` | map<string, string or object or [OutputSecretReference](api-types.md#crates_plugin_api_variables_outputsecretreference)> | required |
| `execution` | string | required |
| `timeout_seconds` | integer | required |

Additional fields are rejected.

<a id="plugins_kubernetes_kube_api_connection"></a>

### Connection

`plugins_kubernetes_kube_api_Connection`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `kubeconfig` | string | required |
| `context` | string or null | optional |
| `projects` | string[] | optional |
| `namespaces` | string[] | required |
| `allow_cluster_resources` | boolean | optional |
| `allow_privileged_workloads` | boolean | optional |

Additional fields are rejected.

<a id="plugins_kubernetes_kube_api_identity"></a>

### Identity

`plugins_kubernetes_kube_api_Identity`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `cluster_uid` | string | required |
| `namespace_uid` | string | required |
| `version` | string | required |

<a id="plugins_kubernetes_kube_resource_evidence"></a>

### Evidence

`plugins_kubernetes_kube_resource_Evidence`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `uid` | string | required |
| `resource_version` | string | required |
| `generation` | integer | required |
| `observed_generation` | integer | required |
| `ready` | boolean | required |

<a id="plugins_kubernetes_kube_resource_intent"></a>

### Intent

`plugins_kubernetes_kube_resource_Intent`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `owner` | string | required |
| `fence` | integer | required |
| `namespace` | string | required |
| `object` | any JSON | required |

<a id="plugins_kubernetes_lib_evidence"></a>

### Evidence

`plugins_kubernetes_lib_Evidence`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `identity` | [Identity](api-types.md#plugins_kubernetes_kube_api_identity) | required |
| `fence` | integer | required |
| `resources` | [Evidence](api-types.md#plugins_kubernetes_kube_resource_evidence)[] | required |

Additional fields are rejected.

<a id="plugins_kubernetes_lib_spec"></a>

### Spec

`plugins_kubernetes_lib_Spec`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `target` | [Target](api-types.md#plugins_kubernetes_lib_target) | required |
| `objects` | any JSON[] | required |
| `timeout_seconds` | integer | required |

Additional fields are rejected.

<a id="plugins_kubernetes_lib_target"></a>

### Target

`plugins_kubernetes_lib_Target`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `namespace` | string | required |
| `identity` | [Identity](api-types.md#plugins_kubernetes_kube_api_identity) | required |

Additional fields are rejected.

