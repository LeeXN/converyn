# Remove Kubernetes and retain data

[English](UNINSTALL.md) | [简体中文](UNINSTALL.zh-CN.md)

This removes the cluster installed by this example. Stop its application releases and reconciliation first and retain the original generated installation directory. The tool binds the original Nodes, CA, API endpoint, owner and kubeadm digest; it cannot adopt another cluster. Generation and registration do not execute removal; the final Run mutates hosts.

## Generate, preview and run

```bash
.venv/bin/python scripts/ops/kubernetes/render_uninstall.py \
  --installation .local/kubernetes-deployment \
  --converyn /path/to/current/converyn --output .local/kubernetes-uninstall
converyn validate workflows --file .local/kubernetes-uninstall/workflow
converyn --context qa-op register workflows --file .local/kubernetes-uninstall/workflow
workflow_id=$(jq -r '.[0].workflow_uid' .local/kubernetes-uninstall/sequence.json)
converyn --context qa-op run workflows "$workflow_id" --target default --dry-run
converyn --context qa-op run workflows "$workflow_id" --target default \
  --request-id kubernetes-uninstall-001 --wait --timeout-seconds 7200
converyn --context qa-op get runs run-kubernetes-uninstall-001
converyn --context qa-op logs runs run-kubernetes-uninstall-001
```

## Ordering and retained state

Verify identities and save root-private Kubernetes configuration and storage inventories on every Node first. Reset workers, secondary controls, then the first control. Each step verifies kubelet mounts, container tasks, static manifests, etcd membership and ports; a failure blocks later steps.

Retain NFS/PV backing data, NFS service, containerd and cached images, API proxy, private installation material, and all Converyn credentials, services, state and history. Do not issue Helm uninstall/PVC deletion, blanket iptables/IPVS flushes or removal of other CNIs. Configuration archives are not etcd disaster-recovery snapshots.

## Acceptance and reinstall

Check each host receipt, authenticated Converyn heartbeats, NFS mounts and retained data. Inspect the original Run when results are unknown; network retries reuse its request ID, while a new removal cycle uses a new ID. Reinstall from a new generated directory and new kubeadm materials. Existing NFS data remains without automatic adoption of old PVCs. See [kubeadm reset](https://kubernetes.io/docs/reference/setup-tools/kubeadm/kubeadm-reset/) and [installation](README.md).

## Resume a previously backed-up removal

Inspect the original failed step and retained reset log first. When all checks/backups completed, continue with the original installation, the actual failed Node UID and a new output directory; earlier Nodes are not reset again. Live configs require the original CA. If configs were removed, completion accepts only the original root-private backup after checking hardware, CA, empty static manifests and closed control-plane ports. A new cluster, missing evidence or unexpected remaining etcd membership stops removal.

```bash
.venv/bin/python scripts/ops/kubernetes/render_uninstall.py \
  --installation .local/kubernetes-deployment --converyn /path/to/current/converyn \
  --resume-from control-3 --output .local/kubernetes-uninstall-resume
```

Register, preview and run its generated `workflow/` with a new request ID. Removal first binds Calico cleanup to the original API endpoint and uses pinned crictl to remove CNI-backed sandboxes. Host-network API/etcd sandboxes remain for kubeadm reset so network cleanup finishes before the final API stops. Do not delete backups to bypass checks.
