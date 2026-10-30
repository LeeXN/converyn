# Resources and operations

[English](resources.md) | [简体中文](../../zh-CN/reference/resources.md)

Converyn uses instance capabilities and workflow grants. New identities have no business access. Bindings may restrict workflows and presets; executors control their own runs, maintainers control all runs in scope. CLI, GUI and API share authorization. See [People and workflow access](../how-to/access.md).

| Resource | Responsibility | Maintainer |
| --- | --- | --- |
| Identity | Stable person ID, name, role bindings and enabled state | OP CLI |
| Credential | Certificate/token-hash pair, validity, revocation and overlap deadlines | OP CLI |
| Workflow | Stable workflow ID, current revision and enabled state | OP registration |
| WorkflowRevision | Immutable Playbook, definitions, fixed values, parameter declarations, target presets and provenance | Registration |
| TargetPreset | Allowed host Cluster, external target, or both for a mixed workflow | Source definition |
| Cluster / Node | Versioned managed host inventory, roles, labels and variables; separate enrollment and heartbeat evidence | OP / Agent |
| ExternalTarget | Controller plugin destination and local connection reference | OP |
| Plan / Work | Server-compiled execution snapshot and per-location operations | Compiler |
| Run / Attempt | One execution and each Work's grant, session, progress and result | Runtime |
| Receipt / Log Manifest | Actual evidence, outcome, native observation and log integrity | Execution endpoint |
| Deployment / Drift Policy | Persistent state and observation interval for opted-in reconciliation | Internal |
| Audit / History | Person-attributed commands and retained execution proof | Automatic |

## Target presets, host clusters and external targets

| Concept | Question it answers | Example |
| --- | --- | --- |
| TargetPreset | Which execution destinations should this run use? | Test or production, with a host Cluster or external target fixed by the workflow |
| Cluster | Which hosts can run operations through Agents? | hosts, including nodes, roles and labels |
| ExternalTarget | Which existing platform should the controller operate through an API? | A Kubernetes destination with pinned cluster/namespace identity and a connection reference |

Host deployment selects a preset bound to a host Cluster; Agents write files, manage services or run scripts. Application deployment to an existing Kubernetes cluster selects a preset bound to an external target; the controller updates Deployments through the API and checks status. Mixed workflows can use both hosts and external platforms.

Operators maintain connections, access credentials and target registrations. Executors choose an existing preset, preview and run it. The external-target page shows registrations; workflow details show the selected binding; the preview shows actual steps and locations. See the [Web console](../how-to/web-console.md) for usage and [plugins](../how-to/plugins.md) for connection and plugin constraints.

Controller members are the servers running Converyn consensus and scheduling, shown on the control-plane page.

## Configuration and execution

Playbooks, scripts, long-lived defaults and parameter declarations are maintained in files or GitHub/GitLab. The platform does not edit them or persist execution overrides as defaults. OP registers a complete immutable snapshot from a local directory or Git checkout; controllers never reread a moving source branch during execution.

Only declared fields may be overridden. Bindings target script environment variables or native/plugin business fields. Unknown names, invalid types/ranges, destination changes, interpreter/script changes, execution profiles, plugin identities and credential-reference changes are rejected. Secrets remain fixed-version references.

CLI and GUI select an existing workflow, preset target and allowed parameters. The server compiles the Plan. GUI displays it before execution; CLI previews and submits automatically or uses --dry-run. Preview is not approval or proof of execution. Source/target changes invalidate old previews. Network retries reuse the request ID; a deliberate rerun uses a new ID.

Executors read authorized Runs/logs and control their own submissions. Cancellation requires stopping or completion evidence and is not rollback. Unknown script outcomes retain their original Run and receipts rather than triggering blind replay.

## Reconciliation

Workflows execute once by default. An explicit reconcile_seconds policy starts observation after successful execution and retains that execution's final snapshot. It does not change stored defaults. Later executions replace desired state; active rollout/repair blocks competing submission. Pause/cancel stops further repair for that generation.

Only reconcilable operations participate. Arbitrary scripts, package installation and reboot are not replayed. System repair Runs retain the original person and source revision.

## Identity and history

Personal certificates and tokens are paired; rotation preserves person ID. Audit automatically records the authenticated person, credential, request, source revision, parameter snapshot and outcome. It records committed state-machine commands, not every HTTP access. Local plan, agent-info and run-script do not automatically enter controller audit. Raw-log storage is independent.

Application/Release/Environment/Delivery and project approval roles are no longer execution prerequisites. Old write APIs are removed. Use a fresh instance; old instances and backups remain with their original version, without automatic import or ownership takeover.

Compared with Wheeljack, Playbook corresponds to Recipe and Run to Task; Work, Attempt and Receipt split SubTask/HostTask planning and evidence responsibilities.

See [getting started](../how-to/installation.md), [credentials](../how-to/certificates.md), [GUI](../how-to/web-console.md), [CLI](cli.md) and [API](api-reference.md).

## OP configuration conventions

Presets may fix node_uids, roles and labels, intersected with step selectors; an empty intersection never becomes all Nodes. Ownership defaults to workflow × preset. OP may explicitly set a shared owner_uid for related stage workflows; execution overrides cannot change it or take another owner's resources.

The project field in execution/plugin contracts is the fixed internal default namespace, without Project resources or human project grants. ExternalTarget defaults it automatically. Workflow UID register is reserved. Packages are limited to 1 MiB and retained revision content to 32 MiB.

Workflow-owned `values`, `node_values` and `immutable` declarations support execution-only YAML/JSON files, `--set` overrides and GUI K/V inputs. See the [workflow values example](../../../example/workflow/README.md).
