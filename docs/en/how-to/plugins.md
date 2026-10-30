# Using and developing plugins

[English](plugins.md) | [简体中文](../../zh-CN/how-to/plugins.md)

Plugins are linked through compile-time features, not uploaded executables. `full` includes the four plugins below. Inspect actual binaries with `get plugins --local`; `get plugins ID` inspects a controller plugin. Plugins share workflow registration, plans, grants, receipts, and audit.

For direct deployment without workflow JSON assembly, start with the [native plugin examples](../../../example/plugins/README.md). The generation commands below explain the lower-level interfaces.

## Capabilities and execution locations

| Plugin ID | Actions | Location/purpose |
| --- | --- | --- |
| `converyn.docker` | `ensure_container` | Node: digest-pinned image, container configuration, health |
| `converyn.linux-host` | `sysctl`, `select_kernel` | Node: kernel parameters and grubby selection of installed kernels |
| `converyn.kubernetes` | `apply` | Controller: existing external Kubernetes API/readiness |
| `converyn.kubeadm` | init, join, upgrade_control_plane, upgrade_secondary_control_plane, upgrade_node, check, drain, uncordon | Fixed Bash expanded for Nodes; does not accept `compile plugins` |

## What each command does

| Command | Input → output | Business changes? |
| --- | --- | --- |
| `get plugins --local` | Descriptors; `-o json` includes full identities/digests | Read only; explicitly inspects this binary, without loading a client context |
| `compile plugins` | YAML/JSON spec → `kind: plugin` operation | Local compilation only; no execution/registration |
| `prepare plugins` | Preparation input → private materials directory | Operator-side preparation, not deployment; used by kubeadm |
| `expand plugins` | Plugin input → manifest/playbook JSON and definition text | Expands fixed scripts, without execution/registration; used by kubeadm |

Embed generated results in a complete workflow, validate/register, then execute with `run`. Use fresh output directories and keep private inputs out of Git. Compare local/controller descriptors, and match Agent builds for Node plugins:

```bash
converyn -o json get plugins --local
converyn --context qa-op -o json get plugins
```

## Host plugins: register and run

Enrolled nodes trust approved calls without per-node plugin grants. Use the [complete plugin workflow examples](../../../example/plugins/README.md), select actual nodes, then validate, register, preview and run. Matching builds fix plugin identities automatically; incompatible builds are rejected before execution.

## Docker: pinned images and centralized Secrets

Use the [Docker workflow example](../../../example/plugins/docker.yaml), pin the image digest and select the actual node platform. The approved spec declares names, mounts and images without local plugin grants.

For private registries use `credential_ref: {secret: registry-login, version: v1}`. Register a Docker config.json containing only auths and the requested registry. The task uses a temporary private file and removes it on completion. Keep passwords outside ordinary environment values.

## kubeadm: prepare materials and expand scripts

Preparation and expansion are separate. Install/upgrade preparation needs version-specific endpoint/CA inputs. See the [infrastructure example](../../../example/kubernetes/README.md) for complete inputs and distribution paths. Transfer generated private materials through a protected channel; never embed CA keys, bootstrap tokens, or administrator kubeconfigs in workflows.

Start with the [check input template](../../../example/plugins/kubeadm-check-input.yaml) to understand expansion. Replace paths, CA_SHA256, and NODE_NAME. Select a supported version profile and declare `execution: host`. The following only expands scripts, without connecting to Kubernetes:

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

`playbook.json` contains steps only, not a complete workflow. `manifest.json.definitions` already inlines expanded scripts by digest, so the generated workflow needs no `scripts` mapping. Review selectors, origin, profile, environment, paths, and grants before registration/dry-run/execution. Init/join/upgrade also need `BINARY_SHA256`, `CONFIG_PATH`, and `CONFIG_SHA256`. Shared script bodies are bundled into every definition; do not copy common.sh separately.

## External plugins and parameter boundaries

Kubernetes runs on controllers and does not use Node plugin grants. It requires `server.json.plugin_connections`, private kubeconfig, a registered ExternalTarget, and a workflow `external_target_uid`. Follow [external Kubernetes](external-kubernetes.md). Mixed workflows also declare an inventory, `node_group_uid` and `hosts`.

Bind business inputs through `parameters.bindings` to allowed spec fields; the complete modified call is revalidated. Destination, plugin identity, credentials, owner, and resource identity remain fixed. Operation generation, registration, preview, and actual authorization are separate stages; generated calls are not permission grants.

## Development and recovery contracts

See [plugin-api](../../../crates/plugin-api) and the [SDK example](../../../plugins/example). Implement description, compilation, validation, local authorization, execution, and evidence verification; script packages implement prepare/expand. Register new plugins in runtime and compile with the appropriate feature. Copying a .so to a server is insufficient.

Pure target/call validation and binding are recorded in a Journal before consensus. Replay uses frozen results. Pure contracts must not perform external I/O; Nodes/controllers make actual requests. Unknown outcomes retain grants and receipts. Reconcilable operations may re-observe; other operations need administrator investigation, not blind replay.

See [authoring](workflows.md) for script permissions, [API types](../reference/api-types.md) for structures, and [CLI](../reference/cli.md) for command options.



Directory preparation accepts generic normalized absolute paths without an application-specific directory allowlist. For example, in a native workflow:

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

Enrolled nodes execute approved host operations without node permission fragments.
