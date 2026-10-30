# Usage guide

[English](runs.md) | [简体中文](../../zh-CN/how-to/runs.md)

Complete [installation](installation.md) first. This page covers daily operations; see the generated [CLI reference](../reference/cli.md) for all flags and [resources](../reference/resources.md) for responsibilities. Replace `RUN_ID`, `REVISION` and `WORK_ID` with query results.

## Select an identity and environment

```bash
converyn config import /secure/qa-alice/config.json --name qa-alice
converyn config get-contexts
converyn config use-context qa-alice
converyn config current-context
converyn --context qa-op status
converyn config view
converyn config export --output /secure/alice-portable.json
```

First import selects the context; later imports preserve selection unless `--use` is given. `--context` applies only to one invocation. `--config FILE` or `CONVERYN_CONFIG` selects a different file. `view` redacts keys/tokens; `export` creates a private single-context file and refuses overwrites. Never commit personal configuration.

New identities have no business access. Workflow/preset grants determine visible workflows, runs and logs. Executors control their own runs; maintainers control all runs in scope. Platform operations use separate capabilities. There is no approval stage.

## Preview, values and execution

```bash
converyn get workflows
converyn get workflows demo
converyn run workflows demo --target qa --set VERSION=two --dry-run
converyn run workflows demo --target qa --set VERSION=two --wait
```

`run workflows demo` submits a registered workflow; the workflow ID is positional. Submission returns immediately by default. Add `--wait` to wait in terminals, pipelines or JSON mode, with a default timeout of 1800 seconds. `--timeout-seconds` requires `--wait`; `--dry-run` and `--wait` are mutually exclusive. Multiple presets require `--target`; a single preset can be omitted. Preview validates targets, values and the plan without execution; target state may change afterwards.

Override example defaults for this execution:

```bash
converyn run workflows demo -f site.yaml -f temporary.json \
  --set image.tag=v2 --set-string VERSION=123 --wait
```

Prepare `site.yaml` and `temporary.json` as YAML/JSON objects. Precedence: workflow defaults → Node defaults → `-f` files left to right → `--set` → `--set-string`. Objects merge recursively; arrays, scalars and null replace whole values. Use `--set-string` for numeric-looking version strings. `immutable` fields cannot change. Overrides do not update future defaults. See [authoring](workflows.md).

## Retry the original request

```bash
converyn run workflows demo --target qa --set VERSION=two --request-id qa-change-001 --wait
```

`qa-change-001` is an example. Give each new business execution a new ID; retries keep **the same ID and request content**. CLI records the request and Run ID before submission and persists the original request in `requests/` beside the client config. Override with `--request-dir` or `CONVERYN_REQUEST_DIR`. Keep private execution records outside Git.

Disconnection, 503 or client exit does not prove that execution never started. Inspect the recorded Run before retrying the original request. Do not issue a new ID to bypass uncertainty. Reusing an ID with different content is rejected.

## Inspect tasks, nodes and logs

```bash
converyn status
converyn get runs
converyn get runs --status needs_attention
converyn get runs RUN_ID
converyn get runs RUN_ID -o wide
converyn watch runs RUN_ID --timeout-seconds 600
converyn --context qa-op get nodes
converyn --context qa-op get nodes node-a
converyn logs runs RUN_ID --step verify --node node-a --tail 200
converyn logs runs RUN_ID --step verify --node node-a --stream stderr
converyn logs runs RUN_ID --step verify --node node-a --follow
converyn logs runs RUN_ID --full --output-dir ./run-logs
```

Run details show step, node, operation, status, exit code, reason and copyable log commands. `-o wide` includes full Work IDs. Online Nodes have fresh heartbeats; this does not establish task success. `get` retrieves all pages by default. For large history select one page with `--limit 50 -o json`, then pass `next_cursor` through `--cursor`, keeping filters unchanged.

Logs default to the last 100 lines per stream. Follow resumes from byte cursors; Ctrl+C stops viewing only. Full reads verify saved bytes, and exports refuse to overwrite files. Without an available shared HA log backend, default reads may explicitly fall back to receipt tails; full reads fail. See [operations](maintenance.md) for the 8 MiB per-stream limit and retention. Native operations may expose structured receipts without text logs.

In the browser, open **Steps and scripts** to inspect registered definitions without previewing a run. The preview under **Run configuration** shows scripts rendered with that run’s variables. Task details open in a wide dialog; closing it restores the originating row and scroll position. Logs keep the reading position by default, with an optional **Follow latest output** switch. Storage failures identify receipt-tail fallback explicitly. Diagnostics retain error codes and request IDs, and retryable failures resume at the last accepted byte offset after recovery.

Node lists collapse additional IP addresses. Asset and resource status show freshness separately from completeness. **Data truncated** means an observation exceeded its size limit; it does not mean the disk is full. Audit defaults to actions and failures, with separate views for node status reports and all records. Filtering never deletes audit records.

Exit codes for `watch runs` and `run workflows --wait`:

| Code | Meaning | Next action |
| --- | --- | --- |
| 0 | Success | Verify business acceptance conditions |
| 2 | Failed, canceled or superseded | Inspect original Run and receipts |
| 3 | Operator attention required | Administrator checks original Work and effects |
| 124 | Client wait timed out | Task continues; query or wait again |

## Pause, resume, cancel and resolve

Read the latest `revision` using `get runs RUN_ID`. Pause/resume use revisions to prevent concurrent overwrites:

```bash
converyn pause runs RUN_ID --expected-revision REVISION
converyn get runs RUN_ID
converyn resume runs RUN_ID
converyn cancel runs RUN_ID
```

Read the new revision before resuming. Pause stops new grants; previously granted work retains authorization. Cancellation requires actual stop/completion evidence and **does not roll back** effects. OP authors an explicit rollback workflow when needed.

For uncertainty, an administrator examines the original intent, receipts and destination, then uses the full Work ID:

```bash
converyn --context qa-op resolve runs RUN_ID --work-id WORK_ID \
  --status succeeded --evidence 'Verified original receipt and actual state; linked incident record'
```

Choose `succeeded`, `failed` or `canceled` from evidence. Resolution does not rerun the script.

## Common administration

| Task | Command |
| --- | --- |
| Register/update a workflow | `validate workflows --file FILE`, then `register workflows --file FILE` |
| Stop new executions | `disable workflows demo`; existing Runs continue |
| Inspect people/credentials | `get identities`, `get credentials` |
| Inspect control plane/host groups | `get members`, `get node-groups` |
| Inspect audit | `get audit` |
| Export logical backup | `admin backup --output /secure/qa-backup.json` |
| Inspect plugins/targets | `get plugins`, `get targets` |
| Prepare browser login | `auth gui-login --output-dir /secure/gui-login` |

`update RESOURCE --file FILE` submits a typed operation request with its action, revision and audit fields. Use a named OP context for administration. See [certificates](certificates.md) for credentials and [operations](maintenance.md) for backups/upgrades. Browser login packages contain instructions and CA by default; `--with-credentials` explicitly includes private credentials and requires protected delivery.

## Local validation and plugin inspection

```bash
converyn get plugins
converyn get plugins --local
converyn validate workflows --file example/workflow
```

`get plugins` reads the selected controller's capabilities. `--local` inspects the current binary. Local validation, compilation, preparation and expansion ignore environment client configuration and reject explicit `--config` or `--context`. Use matching plugin builds on the operator, controllers and Agents.
