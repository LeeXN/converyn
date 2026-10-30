# Install Kubernetes and foundation services

[English](README.md) | [简体中文](README.zh-CN.md)

Install containerd, kubeadm Kubernetes, Calico, NFS CSI, cert-manager, Traefik, Harbor and monitoring in one Converyn Run. The workflow supports one or three control nodes plus workers. Use blank Ubuntu 24.04 or prepared Rocky Linux 8/amd64 Nodes with at least 4 GiB RAM and 30 GiB free disk each; cross-worker verification requires two workers. Converyn controllers and Kubernetes control nodes have independent roles.

## Read the workflow

| File | Purpose |
| --- | --- |
| [workflow.yaml](workflow.yaml) | Native ordered stages; directly usable in the prepared directory |
| [cluster.yaml](cluster.yaml) / [cluster-ha.yaml](cluster-ha.yaml) | Single/HA environment inputs; replace UIDs, addresses and service settings |
| [stages.yaml](stages.yaml) | Preparation recipe: template files, kubeadm actions and stage inputs |
| [sources.lock.json](sources.lock.json) | Reviewed component versions, sources and checksums |
| [configs/](configs) / [scripts/](scripts) | Host/Helm configuration and real deployment/verification scripts |

The sequence is preflight → runtime → initial control → serial secondary controls → workers → networking → storage → certificates → ingress → registry → monitoring → functional verification. All Nodes finish a stage before the next begins. Default NodePorts are HTTP 30080 and HTTPS 30443.

## Prepare fixed inputs

Use a `full` binary and a named OP. Complete [Converyn installation](../../docs/en/tutorials/quickstart.md), enroll Nodes, then run from the repository root. A single-control inventory is shown below; copy `cluster-ha.yaml` for HA. Use new output paths for changed inputs.

```bash
uv venv .venv
. .venv/bin/activate
uv pip install -r scripts/ops/kubernetes/requirements.txt
mkdir -p .local
cp example/kubernetes/cluster.yaml .local/kubernetes.yaml
# Edit real Node UIDs, hostnames, addresses, endpoint, NFS and password file paths.
python3 scripts/ops/kubernetes/prepare.py --output .local/kubernetes-assets
python3 scripts/ops/kubernetes/render.py \
  --config .local/kubernetes.yaml --assets .local/kubernetes-assets/artifacts.json \
  --converyn /absolute/path/converyn --output .local/kubernetes-deployment
```

The local tools download/check assets and prepare private kubeadm materials; they do not install anything on Nodes. [Preparation details](PREPARATION.md) cover Rocky kernel/cgroup requirements, host/bootstrap commands, HA routing, existing NFS, artifact HTTPS serving, exact Agent grants and private file distribution. **Complete those steps before deployment.** Keep the CA/key, bootstrap tokens and password files private; password settings are paths, not values.

Preparation produces `.local/kubernetes-deployment/workflow/` with `workflow.yaml`, `inventory.yaml`, `artifacts.yaml` and readable `prepared/` scripts/configurations. This is an ordinary native source directory; registration resolves file digests and packages definitions. The private kubeadm kit is a sibling directory and is excluded from registration. Create the platform node group from node-group.json before registering the workflow; Python only prepares source artifacts.

## Deploy

Enrolled nodes execute approved host operations without node permission fragments.

```bash
converyn --context qa-op update node-groups --file .local/kubernetes-deployment/node-group.json
converyn validate workflows --file .local/kubernetes-deployment/workflow
converyn --context qa-op register workflows --file .local/kubernetes-deployment/workflow
workflow_id=$(jq -r '.[0].workflow_uid' .local/kubernetes-deployment/sequence.json)
converyn --context qa-op run workflows "$workflow_id" --target default --dry-run
converyn --context qa-op run workflows "$workflow_id" --target default \
  --request-id kubernetes-install-001 --wait --timeout-seconds 7200
```

The workflow has a stable owner derived from your environment inputs. Preview checks planned work; actual execution checks grants, sources, files and target state. Retain the request ID for network retries and inspect the original Run after uncertain results.

## Verify and recover

```bash
converyn --context qa-op get runs run-kubernetes-install-001
converyn --context qa-op logs runs run-kubernetes-install-001 --follow
```

The last stage verifies Node roles, DNS, cross-worker HTTP, network policy, persistent storage, actual metrics and TLS OCI push/pull. Evidence remains at `/opt/converyn-example/evidence` on the first control node. Read successful receipts and business results; service status alone is insufficient.

Do not rerun full initialization/join to repair an installed cluster. Inspect the original Run, preserve the private kit/Helm history/PVCs and prepare only affected recovery steps. Join tokens expire after two hours; review refreshed materials before registration. Use the original installation directory for [removal workflows](UNINSTALL.md), which retain NFS data and Converyn state.

For advanced image archives, existing Cluster snapshots and configuration editors see [operator tools](../../docs/en/how-to/kubernetes-tools.md). The environment is a lab profile with an existing NFS server, private CA and NodePorts. To publish an application to an existing cluster, use [external Kubernetes](../external-kubernetes/README.md).

Default inventories use separate platform node groups so they do not replace the quickstart group. When reusing an approved group through `--node-group-snapshot`, skip group creation and confirm it already contains every selected Node.
