# Converyn HTTP API

[English](../../en/reference/api-reference.md) | [简体中文](../../zh-CN/reference/api-reference.md)

Generated from [OpenAPI](../../schema/openapi.json). See [types](api-types.md), [resource responsibilities](resources.md), [usage](../how-to/runs.md) and [workflow authoring](../how-to/workflows.md).

## Identity and execution

Business APIs use `/api/v1`, client certificates, `Authorization: Bearer <personal token>` and `x-converyn-cluster`. New people have no business grants. Platform roles and workflow/preset bindings determine capabilities. Runs, logs and archives follow their original execution scope. See [workflow access](../how-to/access.md). There is no password login or approval step.

OP maintains source files or Git checkouts and registers immutable snapshots with CLI. Execution accepts a workflow revision, preset target, declared overrides and request ID; the server compiles the Plan and rechecks its digest and target snapshot. Legacy catalog, approval, script upload and direct Plan upload endpoints are removed.

Preview is not execution. Follow the returned Run and actual receipts. Overrides apply to this execution; opted-in reconciliation retains its snapshot. Retries reuse the original request ID.

## Consistency, limits and logs

Reads confirm quorum; followers preserve the authenticated identity. Business changes and command audit commit together. Audit automatically records the person, credential, workflow revision and outcome; it is not a complete HTTP access log.

Bodies are limited to 4 MiB, workflow packages to 1 MiB, definitions to 64 KiB and final parameter snapshots to 256 KiB. Workflow request IDs have at most 124 characters. `_ms` uses Unix milliseconds; `_seconds` uses seconds.

Errors contain code, message, retryable and request_id. Back off on 429. After 503 or disconnection, query the original request before retrying. Logs use 64 KiB chunks. Streaming reads revalidate identity. Raw logs are separate from audit and logical backups.

## Routes

### `GET /`

The HTTPS listener still checks the client certificate.

No JSON request body.

Successful response HTTP 200:

Media type `text/html`.

Stream/text response; honor Content-Type.

### `GET /api/v1/access`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `revision` | integer | required |
| `groups` | map<string, [Group](api-types.md#crates_core_identities_access_group)> | required |
| `bindings` | map<string, [Binding](api-types.md#crates_core_identities_access_binding)> | required |
| `roles` | [Role](api-types.md#crates_core_identities_access_role)[] | required |
| `workflows` | object[] | required |

### `POST /api/v1/access`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/access/check`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `action` | query | string | optional |
| `workflow` | query | string | optional |
| `preset` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `allowed` | boolean | required |
| `action` | [Action](api-types.md#crates_core_identities_access_action) | required |
| `workflow` | string or null | required |
| `preset` | string or null | required |
| `reason` | string | required |

### `GET /api/v1/access/me`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `revision` | integer | required |
| `bindings` | [Binding](api-types.md#crates_core_identities_access_binding)[] | required |
| `platform` | boolean | required |
| `access` | boolean | required |
| `super_admin` | boolean | required |

### `GET /api/v1/agents/{node}/attempts/{run}/{work}`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `run` | path | string | required |
| `work` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `output_key` | [PublicKey](api-types.md#crates_core_model_outputs_publickey) or null | required |
| `work` | [Work](api-types.md#crates_core_model_domain_work) | required |
| `attempt` | [Attempt](api-types.md#crates_core_orchestration_machine_types_attempt) | required |
| `run_status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `project` | string or null | required |
| `owner_uid` | string | required |

### `POST /api/v1/agents/{node}/commands`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

Request body:

Media type `application/json`.

Discriminator `type=preparation`

| Field | JSON type | Required / default |
| --- | --- | --- |
| `change` | [Change](api-types.md#crates_core_orchestration_preparation_change) | required |
| `type` | `"preparation"` | required |

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/agents/{node}/heartbeat`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/agents/{node}/log-storage`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `enabled` | boolean | required |
| `available` | boolean | required |
| `store_id` | string or null | required |

### `POST /api/v1/agents/{node}/logs/{run}/{work}/{stream}`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `run` | path | string | required |
| `work` | path | string | required |
| `stream` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant` | string or null | optional |
| `offset` | integer | required |
| `data` | string | required |
| `sha256` | string | required |
| `complete` | boolean | optional |
| `missing` | boolean | optional |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `next_offset` | integer | required |

### `POST /api/v1/agents/{node}/logs/{run}/{work}/{stream}/live`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `run` | path | string | required |
| `work` | path | string | required |
| `stream` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant` | string or null | optional |
| `offset` | integer | required |
| `data` | string | required |
| `sha256` | string | required |
| `complete` | boolean | optional |
| `missing` | boolean | optional |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `next_offset` | integer | required |

### `POST /api/v1/agents/{node}/renew`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string | required |
| `csr_pem` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) or null | required |

### `POST /api/v1/agents/{node}/renew/activate`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |

### `GET /api/v1/agents/{node}/runs/{run}/work/{work}/artifacts/{sha}`

Only the current Node identity and its ongoing Run assignment; supports byte-range resume.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `run` | path | string | required |
| `work` | path | string | required |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |
| `Range` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/octet-stream`.

Stream/text response; honor Content-Type.

Successful response HTTP 206:

Media type `application/octet-stream`.

Stream/text response; honor Content-Type.

### `GET /api/v1/agents/{node}/runs/{run}/work/{work}/inputs`

Only the matching current Node identity and committed grant; human administrators cannot read plaintext. Responses are not cacheable.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `run` | path | string | required |
| `work` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `values` | map<string, string> | required |
| `redactions` | integer[][] | required |

### `GET /api/v1/agents/{node}/scripts/{sha}`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sha256` | string | required |
| `content` | string | required |

### `GET /api/v1/agents/{node}/work`

Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |
| `after` | query | integer | optional |
| `wait_ms` | query | integer | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `cursor` | integer or null | required |
| `node_uid` | string | required |
| `work` | object[] | required |
| `artifact_pins` | map<string, integer> | required |
| `maintenance_until_ms` | integer or null | required |

### `GET /api/v1/artifacts`

Platform-operator permission; files are registered after durable verification on all current voters.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `artifacts` | object[] | required |

### `POST /api/v1/artifacts/{sha}/chunks`

Platform-operator permission; files are registered after durable verification on all current voters.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `size` | query | integer | required |
| `offset` | query | integer | optional |

Request body:

Media type `application/octet-stream`.

Binary chunk; upload using the declared length and offset.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `next_offset` | integer | optional |

### `POST /api/v1/artifacts/{sha}/complete`

Platform-operator permission; files are registered after durable verification on all current voters.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `size` | query | integer | required |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/audit`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | integer | optional |
| `limit` | query | integer | optional |
| `stream` | query | string | optional |
| `view` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `records` | any JSON[] | required |
| `stream` | string | required |
| `view` | string | required |
| `next_cursor` | integer or null | required |

### `GET /api/v1/connections`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `connections` | object[] | required |

### `GET /api/v1/controlplane/backup`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/vnd.converyn.backup.v2`.

Stream/text response; honor Content-Type.

### `POST /api/v1/controlplane/init`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

Type: map<string, object>.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `initialized` | boolean | required |

### `GET /api/v1/controlplane/members`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `membership` | [RaftStoredMembership](api-types.md#raftstoredmembership) | required |
| `config_fingerprint` | string | required |
| `voters` | integer[] | required |
| `operations` | [Catalog](api-types.md#crates_core_orchestration_controlplane_catalog) | required |
| `replication` | map<string, [RaftLogId](api-types.md#raftlogid) or null> or null | required |

### `POST /api/v1/controlplane/members`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/controlplane/metrics`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `consistency` | `"local"` | required |
| `metrics` | [RaftMetrics](api-types.md#raftmetrics) | required |
| `writes` | object | required |

### `GET /api/v1/controlplane/prometheus`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `text/plain`.

Stream/text response; honor Content-Type.

### `POST /api/v1/controlplane/snapshot`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 202:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `requested` | boolean | required |

### `GET /api/v1/credentials`

Named administrator; bootstrap credentials may only create the first administrator. Responses never expose private keys or tokens.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `credentials` | object[] | required |

### `POST /api/v1/credentials`

Named administrator; bootstrap credentials may only create the first administrator. Responses never expose private keys or tokens.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/credentials/{id}`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `credential` | object | required |
| `storage` | map<string, string> | required |

### `GET /api/v1/deployments/{id}`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

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

### `GET /api/v1/deployments/{id}/drift`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `policy` | [Policy](api-types.md#crates_core_orchestration_drift_policy) or null | required |

### `GET /api/v1/executions`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `executions` | map<string, [Execution](api-types.md#crates_core_orchestration_workflow_execution)> | required |

### `GET /api/v1/history`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `records` | map<string, [Record](api-types.md#crates_core_orchestration_history_record)> | required |
| `keep_runs` | integer | required |
| `definition_hot_bytes` | integer | required |
| `configuration_revision` | integer | required |
| `audit` | any JSON[] | required |
| `next_cursor` | string or null | required |
| `run_sequence` | integer | required |

### `POST /api/v1/history`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/identities`

Named administrator; bootstrap credentials may only create the first administrator. Responses never expose private keys or tokens.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `identities` | map<string, [Identity](api-types.md#crates_core_identities_personnel_identity)> | required |

### `POST /api/v1/identities`

Named administrator; bootstrap credentials may only create the first administrator. Responses never expose private keys or tokens.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/identity/bootstrap`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `id` | string | required |
| `node_uid` | string | required |
| `token_sha256` | string | required |
| `csr_pem` | string | required |
| `ttl_seconds` | integer | required |
| `recover` | boolean | optional |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `grant` | [Bootstrap](api-types.md#crates_core_identities_registration_bootstrap) | required |

### `POST /api/v1/identity/enroll`

One-use Node enrollment credentials.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request_id` | string | required |
| `grant_id` | string | required |
| `node_uid` | string | required |
| `hardware_sha256` | string | required |
| `token_sha256` | string | required |
| `csr_pem` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `certificate` | [Signed](api-types.md#crates_core_identities_certificate_signed) or null | required |

### `GET /api/v1/identity/me`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `capabilities` | object | required |
| `principal` | string or null | required |
| `identity` | [Identity](api-types.md#crates_core_identities_personnel_identity) or null | required |
| `credential` | object or null | required |

### `GET /api/v1/identity/nodes`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `nodes` | object[] | required |
| `next_cursor` | string or null | required |

### `POST /api/v1/identity/retire`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_uid` | string | required |
| `expected_fingerprint` | string | required |
| `evidence` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |

### `GET /api/v1/node-groups`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `node_groups` | map<string, [NodeGroup](api-types.md#crates_core_model_domain_nodegroup)> | required |
| `administration` | [Administration](api-types.md#crates_core_orchestration_administration_administration) | required |
| `resource_owners` | map<string, string> | required |
| `next_cursor` | string or null | required |

### `POST /api/v1/node-groups/commands`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/nodes`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `nodes` | map<string, [NodeObservation](api-types.md#crates_core_orchestration_machine_types_nodeobservation)> | required |
| `next_cursor` | string or null | required |

### `GET /api/v1/nodes/{id}`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `nodes` | map<string, [NodeObservation](api-types.md#crates_core_orchestration_machine_types_nodeobservation)> | required |

### `GET /api/v1/plugins`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `plugins` | [Descriptor](api-types.md#crates_plugin_api_lib_descriptor)[] | required |
| `manifest_sha256` | string | required |

### `GET /api/v1/prototype/agents/{node}/work`

Loopback development interface only.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `node` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-hardware` | header | string | optional |
| `x-converyn-session` | header | string | optional |
| `after` | query | integer | optional |
| `wait_ms` | query | integer | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `cursor` | integer or null | required |
| `node_uid` | string | required |
| `work` | object[] | required |
| `artifact_pins` | map<string, integer> | required |
| `maintenance_until_ms` | integer or null | required |

### `POST /api/v1/prototype/commands`

Loopback development interface only.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/runs`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |
| `workflow` | query | string | optional |
| `preset` | query | string | optional |
| `actor` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `runs` | object[] | required |
| `next_cursor` | string or null | required |

### `GET /api/v1/runs/{id}`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

Type: any JSON.

### `POST /api/v1/runs/{id}/cancel`

The submitter or an administrator; the server and state machine check the authenticated identity.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/runs/{id}/pause`

The submitter or an administrator; the server and state machine check the authenticated identity.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `paused` | boolean | required |
| `expected_revision` | integer | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/runs/{id}/resolve`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `status` | [Status](api-types.md#crates_core_orchestration_machine_types_status) | required |
| `actor` | string | required |
| `evidence` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/runs/{id}/uncertain`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `work_id` | string | required |
| `expected_revision` | integer | required |
| `reason` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/runs/{id}/work/{work}/definition`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `work` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `sha256` | string | required |
| `content` | string | required |
| `rendered_content` | string | required |

### `GET /api/v1/runs/{run}/logs/{work}/{stream}`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `run` | path | string | required |
| `work` | path | string | required |
| `stream` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `offset` | query | integer | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `offset` | integer | required |
| `next_offset` | integer | required |
| `data` | string | required |
| `complete` | boolean | required |
| `expired` | boolean | required |
| `truncated` | boolean | required |
| `source` | string | optional |
| `sha256` | string or null | optional |
| `verified` | boolean | optional |
| `missing_ranges` | [integer, integer][] | optional |
| `condition` | string | optional |

### `GET /api/v1/runs/{run}/logs/{work}/{stream}/download`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `run` | path | string | required |
| `work` | path | string | required |
| `stream` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/octet-stream`.

Stream/text response; honor Content-Type.

### `GET /api/v1/runs/{run}/logs/{work}/{stream}/events`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `run` | path | string | required |
| `work` | path | string | required |
| `stream` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `offset` | query | integer | optional |

No JSON request body.

Successful response HTTP 200:

Media type `text/event-stream`.

Stream/text response; honor Content-Type.

### `GET /api/v1/secrets`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `secrets` | object[] | required |

### `POST /api/v1/secrets`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `name` | string | required |
| `version` | string | required |
| `value` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/security/certificates`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `principal` | string | required |
| `expected_revision` | integer | required |
| `fingerprints` | string[] | required |
| `evidence` | string | required |

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/security/certificates`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `identities` | [Identities](api-types.md#crates_core_identities_identity_identities) | required |

### `GET /api/v1/targets`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `targets` | map<string, [ExternalTarget](api-types.md#crates_core_model_target_externaltarget)> | required |
| `next_cursor` | string or null | required |

### `POST /api/v1/targets/commands`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/workflows`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `after` | query | string | optional |
| `limit` | query | integer | optional |
| `status` | query | string | optional |
| `search` | query | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflows` | object[] | required |
| `next_cursor` | string or null | required |

### `POST /api/v1/workflows/register`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflow` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | required |
| `expected_revision` | integer | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /api/v1/workflows/{id}`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `actions` | object | required |
| `workflow` | [Workflow](api-types.md#crates_core_orchestration_workflow_workflow) | required |
| `definition` | [Revision](api-types.md#crates_core_orchestration_workflow_revision) | required |
| `reconciliation` | object[] | required |

### `POST /api/v1/workflows/{id}/enabled`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `enabled` | boolean | required |
| `expected_revision` | integer | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `POST /api/v1/workflows/{id}/preview`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `workflow_revision` | string | required |
| `target` | string | required |
| `parameters` | map<string, any JSON> | optional |
| `request_id` | string | required |

Additional fields are rejected.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `request` | [Request](api-types.md#crates_core_orchestration_workflow_request) | required |
| `plan` | [Plan](api-types.md#crates_core_model_domain_plan) | required |
| `parameters` | map<string, any JSON> | required |
| `rendered_scripts` | map<string, string> | required |

### `POST /api/v1/workflows/{id}/runs`

An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `id` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

Request body:

Media type `application/json`.

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

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |
| `code` | string | required |
| `run_id` | string or null | optional |
| `revision` | integer or null | optional |

### `GET /assets/app.js`

The HTTPS listener still checks the client certificate.

No JSON request body.

Successful response HTTP 200:

Media type `text/javascript`.

Stream/text response; honor Content-Type.

### `GET /assets/index.css`

The HTTPS listener still checks the client certificate.

No JSON request body.

Successful response HTTP 200:

Media type `text/css`.

Stream/text response; honor Content-Type.

### `GET /assets/version.json`

The HTTPS listener still checks the client certificate.

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `version` | string | required |

### `GET /health/live`

The HTTPS listener still checks the client certificate.

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `live` | boolean | required |
| `stage` | string | required |
| `protocol` | integer | required |
| `plugin_manifest` | string | required |
| `node_id` | integer | required |
| `control_plane` | string | required |
| `pristine` | boolean | required |
| `log_storage` | object or null | required |

### `GET /health/ready`

Administrator.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ready` | boolean | required |

### `POST /internal/raft/append`

Controller peer identity with matching protocol and plugin manifest.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-plugin-manifest` | header | string | required |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `prev_log_id` | [RaftLogId](api-types.md#raftlogid) or null | required |
| `entries` | [RaftEntry](api-types.md#raftentry)[] | required |
| `leader_commit` | [RaftLogId](api-types.md#raftlogid) or null | required |

Successful response HTTP 200:

Media type `application/json`.

object

| Field | JSON type | Required / default |
| --- | --- | --- |
| `Ok` | string or object or object | required |

object

| Field | JSON type | Required / default |
| --- | --- | --- |
| `Err` | any JSON | required |

### `GET /internal/raft/artifacts/{sha}`

Controller peer identity with matching protocol and plugin manifest.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-plugin-manifest` | header | string | required |
| `size` | query | integer | required |
| `Range` | header | string | optional |

No JSON request body.

Successful response HTTP 200:

Media type `application/octet-stream`.

Stream/text response; honor Content-Type.

Successful response HTTP 206:

Media type `application/octet-stream`.

Stream/text response; honor Content-Type.

### `POST /internal/raft/artifacts/{sha}/chunks`

Controller peer identity with matching protocol and plugin manifest.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-plugin-manifest` | header | string | required |
| `size` | query | integer | required |
| `offset` | query | integer | optional |

Request body:

Media type `application/octet-stream`.

Binary chunk; upload using the declared length and offset.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |

### `POST /internal/raft/artifacts/{sha}/complete`

Controller peer identity with matching protocol and plugin manifest.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `sha` | path | string | required |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-plugin-manifest` | header | string | required |
| `size` | query | integer | required |

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `ok` | boolean | required |

### `POST /internal/raft/snapshot`

Controller peer identity with matching protocol and plugin manifest.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-plugin-manifest` | header | string | required |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `meta` | [RaftSnapshotMeta](api-types.md#raftsnapshotmeta) | required |
| `offset` | integer | required |
| `data` | integer[] | required |
| `done` | boolean | required |

Successful response HTTP 200:

Media type `application/json`.

object

| Field | JSON type | Required / default |
| --- | --- | --- |
| `Ok` | object | required |

object

| Field | JSON type | Required / default |
| --- | --- | --- |
| `Err` | any JSON | required |

### `POST /internal/raft/vote`

Controller peer identity with matching protocol and plugin manifest.

Internal protocol or restricted local test interface; see this route's access rules.

| Parameter | Location | Type | Requirement |
| --- | --- | --- | --- |
| `x-converyn-cluster` | header | string | required |
| `x-request-id` | header | string | optional |
| `x-converyn-protocol` | header | `27` | required |
| `x-converyn-plugin-manifest` | header | string | required |

Request body:

Media type `application/json`.

| Field | JSON type | Required / default |
| --- | --- | --- |
| `vote` | [RaftVote](api-types.md#raftvote) | required |
| `last_log_id` | [RaftLogId](api-types.md#raftlogid) or null | required |

Successful response HTTP 200:

Media type `application/json`.

object

| Field | JSON type | Required / default |
| --- | --- | --- |
| `Ok` | object | required |

object

| Field | JSON type | Required / default |
| --- | --- | --- |
| `Err` | any JSON | required |

### `GET /openapi.json`

The HTTPS listener still checks the client certificate.

No JSON request body.

Successful response HTTP 200:

Media type `application/json`.

Type: object.

## Maintenance

`python3 tools/generate_api.py`

`python3 tools/render_api_docs.py --check`
