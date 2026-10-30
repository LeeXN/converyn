# Native workflow directories and ordered labels

The operator machine reads each workflow directory through `workflow.yaml`. UID determines workflow identity; the directory name does not. Existing `validate workflows --file` and `register workflows --file` accept directories. Registration creates an immutable snapshot replicated between controllers; it does not scan or synchronize source directories.

## Directory and manifest

```text
/opt/converyn/my-flow/
├── workflow.yaml
├── inventory.yaml
├── values.yaml
├── scripts/check.sh
└── lib/helper.sh
```

```yaml
uid: my-flow
name: Host validation
inventory: inventory.yaml
values: values.yaml
package:
  include: [lib]
targets:
  qa:
    hosts: all
playbook:
  api_version: converyn/v1
  name: host-validation
  steps:
    - id: check
      phase: rollout
      scope: per_node
      hosts: database
      max_parallel: 1
      operation:
        kind: exec
        script: scripts/check.sh
        timeout_seconds: 30
```

The manifest explicitly references inventory, ordinary values and required files. Registration hashes the actual `scripts/check.sh` content without a separate generator or handwritten digest. `values.yaml` is an object following the [values rules](workflows.md#how-values-reach-scripts). Large binaries, packages and charts use separate artifacts rather than inline script packages.

## Workflow node labels

For example, `inventory.yaml`:

```yaml
nodes:
  - uid: node-a
    address: 192.0.2.10
    hostname: db-a
  - uid: node-b
    address: 192.0.2.11
    hostname: db-b
groups:
  database: [node-b, node-a]
  monitoring: [node-a, node-b]
```

UIDs reference enrolled, non-retired Agents; address and hostname describe business topology and do not replace authentication. Groups are workflow-local labels: nodes may belong to multiple groups and workflows, independently of controller or leader roles. `all` is derived from node order and cannot be redeclared. Every host target declares both `node_group_uid` and `hosts`. Inventory is the only deployment topology; platform membership bounds its allowed nodes and may overlap across groups. See [node groups](node-groups.md).

Initially approving a target, changing its bound node group or renaming it requires platform and workflow maintenance permissions. With an unchanged binding, maintainers can update nodes, addresses, hostnames, groups, ordering and business content within the approved group. Runtime values and `--set` cannot replace inventory or script identity.

Selectors support a complete group, `database[0]`, `database[-1]`, and inclusive `database[0-1]`. Unknown groups and invalid indices fail. Optional empty selections require step `allow_empty: true`. Group selectors cannot be combined with `roles`, `labels` or `node_uids`.

Work is dispatched in list order. `max_parallel: 1` is serial; larger limits bound concurrency without guaranteeing completion order. Directory workflows default to one whole rollout batch. An explicitly smaller `batch_size` must not conflict with group order across the target's rollout batches; preview rejects conflicts. Re-registration and leader changes never reorder an existing Run.

Workflows may reuse nodes. Active Runs reserve their actual execution nodes; disjoint nodes can run concurrently. Resource ownership and control-plane task grants still apply. Node groups bound the allowed nodes; approved workflows declare the operations.

## Scripts and dependencies

```bash
#!/usr/bin/env bash
set -euo pipefail
source "$CONVERYN_PACKAGE_ROOT/lib/helper.sh"
printf 'node=%s\n' "$CONVERYN_NODE_UID"
```

Explicit UTF-8 files and directories in `package.include` form a read-only dependency package exposed through `CONVERYN_PACKAGE_ROOT`. The task working directory remains separately writable. Content-addressed revisions do not overwrite each other. Paths must stay inside the source directory; symlinks and special files are rejected. Each file is at most 64 KiB; the final revision and Plan are each at most 1 MiB. Larger release payloads use the local artifact references below or explicit external sources.

The platform injects `CONVERYN_NODE_UID`, `CONVERYN_RUN_ID`, `CONVERYN_WORK_ID` and `CONVERYN_OWNER_UID`. Local inventory also exposes `CONVERYN_INVENTORY_JSON` with the current node, business address, hostname, labels, ordered nodes and groups. JSON values are data, never Shell source. Scripts publish results using `task emit`; declared inputs bind consumers, with `node: database[0]` selecting one producer.

## Declarative source references

Reference artifact configuration using `artifacts: artifacts.yaml`. `{$ref: /artifacts/archives/charts/artifact}` reads a JSON Pointer in the complete source document; `{$concat: [prefix, {$ref: /values/VERSION}]}` concatenates strings only. Missing or recursive references fail without evaluating expressions, Shell or external hooks.

```yaml
artifacts: artifacts.yaml
playbook:
  api_version: converyn/v1
  name: packages
  steps:
    - id: install
      for_each: /artifacts/packages
      phase: rollout
      scope: per_node
      hosts: all
      max_parallel: 1
      operation:
        kind: package
        manager: {$item: /manager}
        name: {$item: /name}
        version: {$item: /version}
        artifact: {$item: /artifact}
        timeout_seconds: 1800
```

`for_each` expands only the referenced array into stable IDs such as `install-0`; `$item` is a JSON Pointer in the current element. Empty arrays require explicit `allow_empty: true`. This general declaration works with package, file, check and other operations.

### Distribute local artifacts through Converyn

Reference a file relative to the workflow directory from a native Binary or Package operation:

```yaml
operation:
  kind: binary
  path: /opt/example/tool
  mode: 493
  artifact: {$artifact: assets/tool}
```

The same reference can live in artifacts.yaml and be selected with $ref / $item. validate workflows reads actual bytes and computes SHA-256 and size. register workflows uploads local artifacts before registration. Escaping paths, symlinks, empty files and files larger than 10 GiB are rejected. Bytes are kept outside JSON, script text and Raft logs; removing the operation machine's source directory after upload does not affect registered versions.

Frozen references contain sha256 and size. Omitting locations selects Converyn controller delivery. Agents use their existing identity and assigned Run, preserving caching, resume, checksum/size verification and local write authorization. No external file-server configuration is required on Agents.

Registration progress on stderr shows hashing, total confirmed bytes, file counts, the current file, effective upload speed and replica-verification stages. Terminals use a dynamic bar; redirected stderr uses plain text. stdout retains its existing result or JSON shape. A 100% byte count can still await replica verification; files count as complete only after confirmation. Existing payloads are explicitly counted as reused.


Plugins can declare `kind: plugin`, `id`, `action` and `spec` directly. Registration resolves built-in contracts; nodes verify approved control-plane calls without local grant fragments. Text file operations accept `source: files/config.txt` or inline `content`; `files: {step-id: {json: /artifacts}}` writes a fixed JSON document. Artifact credentials belong in Agent configuration, not workflow source.

## Registration, preview and boundaries

```bash
converyn validate workflows --file /opt/converyn/my-flow
converyn --context qa-op register workflows --file /opt/converyn/my-flow
converyn --context qa-op run workflows my-flow --target qa --dry-run
converyn --context qa-op run workflows my-flow --target qa \
  --request-id my-flow-001 --wait
```

`validate` checks local definitions, files and references; requirements lists operations and Secret references without checking remote readiness. Before creating a Run, controllers check centralized Secrets and advertised node plugins. Agents execute approved operations without local permission fragments.

Secrets are encrypted in the control plane; workflows retain only names and versions. Keep private content out of package.include. Registered file snapshots survive removal of operator source directories; re-register changes while existing Runs retain their original snapshots.

[Runnable directory example](../../../example/workflow-directory/README.md)

Source references are resolved at registration. Mark values used for fixed artifact identity, paths or Secret selectors as immutable; use parameters.bindings when an operation needs a runtime override. Inventory node values are defaults, workflow node_values take precedence, and explicit runtime overrides apply last. An entirely skipped plan is rejected as no_work_selected.

Exec operations may declare fixed_environment for immutable step options such as a shared wrapper entry. Ordinary values, node defaults, --set, parameter bindings and step output bindings cannot replace these names. Do not duplicate a name in environment and fixed_environment. Use ordinary values for intended runtime inputs.

URI/fragment $ref values such as #/definitions/type remain payload data. Wrap a value in {$literal: ...} when it contains source-like directives that must not be interpreted.

The execution work_root is supervisor-owned: business/native write grants must not cover it or its ancestors. Keep task storage separate from writable application directories so other tasks cannot alter cached dependency packages.

Disposable Agent E2E tests require root to verify dropping tasks to a separate UID; CI invokes the selected Python with sudo while keeping the configured tool PATH. These tests replace service installation and never target the existing fleet.
