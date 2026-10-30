# Kubernetes operator tools

[English](kubernetes-tools.md) | [简体中文](../../zh-CN/how-to/kubernetes-tools.md)

Source examples, executed Shell and configurations live in [example/kubernetes](../../../example/kubernetes/README.md). Python here downloads and prepares fixed inputs; it is not Agent business code. Regressions live in [scripts/tests/kubernetes](../development/testing.md).

Register the prepared native workflow directory and run it with `converyn register workflows` and `converyn run workflows`; see the example for the full command sequence.

## Operator tools

Run these tools from the repository root on the operator machine. `prepare.py` downloads artifacts and `render.py` prepares the native workflow directory and private materials. These tools do not contact the Converyn control plane.

### prepare.py: download artifacts

Reads [sources.lock.json](../../../example/kubernetes/sources.lock.json), downloads fixed binaries and Charts over HTTPS, checks their SHA256, and extracts the listed archive members. Requires Python 3 and `curl`.

| Option | Required | Description |
| --- | --- | --- |
| `--output DIR` | Yes | Asset directory; reusable for identical inputs |
| `--images-archive FILE` | No | Operator-supplied Docker/OCI tar archive, up to 10 GiB |

Output under `DIR`:

| Path | Contents |
| --- | --- |
| `downloads/` | Downloaded source files and extracted binaries |
| `blobs/SHA256` | Content-addressed files copied into the native workflow for managed distribution |
| `artifacts.json` | Component versions and each artifact's target path, SHA256, size, mode and source checksum |

Matching downloads and blobs are reused. Changed inputs require a new output directory. Downloads use four workers and `curl --retry 3`; a checksum mismatch stops preparation. The optional image archive is validated, hashed and copied into `blobs/` for import during the runtime stage.

### render.py: generate deployment files

Reads the cluster inventory, asset manifest, [stages.yaml](../../../example/kubernetes/stages.yaml), [configs/](../../../example/kubernetes/configs) and [scripts/](../../../example/kubernetes/scripts). Calls local `converyn prepare plugins`, `expand plugins` and `compile plans` commands. Requires Python 3, [requirements.txt](../../../scripts/ops/kubernetes/requirements.txt) and a Converyn binary built with `full`.

| Option | Required | Description |
| --- | --- | --- |
| `--config FILE` | Yes | Cluster configuration in YAML or JSON; start with `cluster.yaml` or `cluster-ha.yaml` |
| `--assets FILE` | Yes | `artifacts.json` from `prepare.py` |
| `--converyn FILE` | Yes | Path to the Converyn executable |
| `--output DIR` | Yes | New output directory, created with mode 0700 |
| `--kubeadm-kit DIR` | No | Existing `private-kubeadm/` kit for addon repair; validates CA/configuration digests, endpoint and networks |
| `--node-group-snapshot FILE` | No | Reuse a registered NodeGroup as the allowed scope; inventory retains deployment order |

| Output | Contents |
| --- | --- |
| `workflow/` | Native directory with inventory, prepared file/script sources and parameter bindings; recommended registration entry |
| `plans/STAGE.input.yaml`, `plans/STAGE.input.json` | Native PlanInput for offline `converyn compile plans` |
| `plans/STAGE.json` | Compiled Plan, including work items and digest |
| `playbooks/STAGE.yaml` | Expanded Playbook |
| `definitions/SHA256.txt` | Fixed script and configuration text |
| `workflows/STAGE.json` | Complete workflow snapshots for `register workflows` |
| `node-group.json` | Destination registration requests |
| `private-kubeadm/` | CA, private key, bootstrap tokens and init/join configuration |
| `prepare-kubeadm.json`, `UID-expand.json`, `expanded-UID/` | Local plugin preparation and expansion inputs/outputs |
| `sequence.yaml`, `sequence.json` | Ordered stage IDs, workflow/request IDs and preview digests |
| `public-cluster.json` | Copy of the supplied cluster configuration |

Generation validates one or three control nodes, at least one worker and 2–20 total Nodes. Cross-worker acceptance requires at least two workers. Outputs contain one installation Workflow with 11 child stages for a single control node or 12 for HA. Keep the generated directory private; transfer the files listed in the example's [preparation guide](../../../example/kubernetes/PREPARATION.md) to their designated Nodes.

`--node-group-snapshot` binds the approved group revision. Both paths use workflow inventory; platform member order and extra members do not change deployment.

## Image inventory and uninstall generation

`check_image_inventory.py` validates the operator's image archive inventory, pinned kubeadm digest and containerd sandbox. It runs local version/image-list commands, not remote installation acceptance. Supply `--kubeadm FILE --inventory FILE` and optionally `--containerd-config FILE`.

`render_uninstall.py --installation DIR --converyn FILE --output DIR [--resume-from NODE_UID]` reads original installation output and generates removal workflows without registering/executing them. See [uninstall](../../../example/kubernetes/UNINSTALL.md).

Install [requirements.txt](../../../scripts/ops/kubernetes/requirements.txt), then use `python3 scripts/ops/kubernetes/TOOL.py --help` from the repository root. Keep generated material under protected `.local/` directories. Tools locate repository `example/kubernetes/` independently of the working directory; distribute both directories together.
