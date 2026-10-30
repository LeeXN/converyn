# Kubernetes preparation

[English](PREPARATION.md) | [简体中文](PREPARATION.zh-CN.md)

## 1. Prepare operator inputs

The operator needs Python, uv, curl, jq and a full binary. Follow [installation](../../docs/en/how-to/installation.md) for controllers, named OP and Agents; no publisher/project-role setup is required. Python below prepares local inputs, without executing installation.

From the repository root, use the commands in the [deployment guide](README.md) to prepare `.local/kubernetes-deployment/` once. The following sections explain the environment inputs and files to distribute.

Use the Python environment where the requirements were installed. Example `192.0.2.0/24` addresses must be replaced. Node UID, actual hostname and IPv4 address are distinct fields. Use stable `project`, `cluster_uid` and `owner_uid` values for example naming and resource ownership. The first `control_plane: true` node initializes; exactly one or three nodes have that flag. Pod/Service networks must not overlap each other, host networks or actual routes.

`nfs.server`/`share` name an existing export. `harbor_host` is the certificate SAN/Ingress host; the NodePort is explicit. Password fields contain root-private file paths, never password values. `verification_image` is fixed to a digest.

Use a new output directory for each render. For addon repair, pass `--kubeadm-kit ORIGINAL/private-kubeadm` to reuse the original CA and configuration after digest, endpoint and network validation. Prepare a narrow recovery Plan for the affected steps; do not rerun full init/join. Join tokens expire after two hours; refresh expired inputs, review their digests and update the registered workflow snapshot before joining Nodes.

## 2. Prepare hosts, HA routing and NFS

### Preparing Rocky Linux 8

Rocky Linux 8's default 4.18 kernel and cgroup v1 do not meet this example's requirements. Before bootstrap, install and boot a reviewed Linux 5.10 or newer kernel, enable unified cgroup v2 and reboot. Check `uname -r` and `/sys/fs/cgroup/cgroup.controllers`. The minimum comes from [Calico 3.33's requirements](https://docs.tigera.io/calico/latest/getting-started/kubernetes/requirements); [Kubernetes cgroup v2](https://kubernetes.io/docs/concepts/architecture/cgroups/) itself requires Linux 5.8 or newer. Both containerd and kubelet use the systemd cgroup driver in this example.

On Rocky 8, `bootstrap-node.sh` installs dependencies with `dnf` and tells NetworkManager to leave Calico interfaces unmanaged. It does not upgrade the kernel, change boot arguments, disable firewalls or change SELinux mode. Explicitly prepare a permissive or disabled SELinux lab host; this example does not install an enforcing container/Calico policy. Preflight still rejects active firewalls, swap, insufficient resources and existing Kubernetes state. NFS bootstrap installs `nfs-utils`, and HA API proxy bootstrap installs HAProxy with the same configuration validation and ownership checks. Installation receipts and functional checks on the actual VMs establish runtime acceptance.

Copy this example to each empty target and run as root:

```bash
sudo bash bootstrap-node.sh
```

This installs host dependencies, prepares directories and persistently disables swap. For the HA sample's per-node local proxy, run on all Kubernetes Nodes before init/join:

```bash
bash bootstrap-api-proxy.sh --print kube-ha.example.test 192.0.2.10 192.0.2.11 192.0.2.12
sudo bash bootstrap-api-proxy.sh kube-ha.example.test 192.0.2.10 192.0.2.11 192.0.2.12
```

Replace the DNS name and control-node addresses. This maps that name to a local proxy at port 16443, which forwards TLS to all three API servers. Clients using this endpoint need their own local proxy. An existing unrelated HAProxy configuration is not taken over. You can instead supply an independently managed HA load-balancer endpoint. Spread control nodes across physical failure domains and test the configured endpoint without rewriting it during a failure.

On a separate or explicitly chosen Ubuntu/Rocky NFS host, use a new export and enumerate all Kubernetes client addresses:

```bash
sudo bash bootstrap-nfs.sh /srv/converyn-nfs NODE1_IP NODE2_IP NODE3_IP
```

Use a new export directory. The lab NFS configuration uses `no_root_squash` and mode 0777; configure storage identities, backups and availability for the deployment environment. Permit the configured API, etcd, kubelet, Calico VXLAN UDP 4789, NFS TCP 2049 and required NodePorts in the host firewalls.

## 3. Enroll and distribute artifacts

Enrolled Agents execute approved workflows; reviewed host scripts declare `execution: host`. No Agent policy edits or dedicated HTTPS artifact server are needed. Keep the preparation directory's `blobs/` beside `artifacts.json` when rendering. The generated native workflow copies verified blobs into its own `assets/` and registration uploads them to Converyn's managed store. Large bytes stay outside the Raft log; Agents download only the artifacts assigned to their Run.

## 4. Provision private kubeadm inputs

Directories are root-owned 0700, files 0600, transferred through a private channel:

| Rendered file | Node path | Placement |
| --- | --- | --- |
| private-kubeadm/ca.crt | /etc/converyn-example/kubeadm/ca.crt | All nodes |
| private-kubeadm/init.yaml | /etc/converyn-example/kubeadm/init.yaml | First control node |
| private-kubeadm/join-control-plane.yaml | /etc/converyn-example/kubeadm/join-control-plane.yaml | Other HA controls |
| private-kubeadm/join-worker.yaml | /etc/converyn-example/kubeadm/join-worker.yaml | Workers |
| private-kubeadm/control-plane-pki/ca.crt and ca.key | /etc/kubernetes/pki/ca.crt and ca.key | First control node only |

Generate two independent random passwords on the first control node and write them to the configured Harbor/Grafana password files, root-owned 0600, as single-line UTF-8 with no surrounding whitespace. They are passed through pipes when creating Kubernetes Secrets, never embedded in public definitions. Protect kubeconfigs, etcd and Helm state too.

Nodes require the official component/Chart image registries. For restricted networks, prepare a complete reviewed Docker/OCI archive from the **same official repositories**, including Calico init images, cert-manager, operator-generated monitoring images and verification images:

```bash
python3 scripts/ops/kubernetes/prepare.py --output .local/kubernetes-offline-assets \
  --images-archive /private/reviewed-images.tar
```

Render using the new asset manifest. The archive is bound by digest and size, distributed to every Node and imported during runtime preparation. Build and verify the complete image archive before preparation, retaining repository digests, image configuration IDs and the archive checksum.

Before rendering, check the core image inventory against the downloaded, checksum-pinned kubeadm and the containerd sandbox image:

```bash
python3 scripts/ops/kubernetes/check_image_inventory.py \
  --kubeadm .local/kubernetes-offline-assets/downloads/kubeadm \
  --inventory /private/images.json
```

The reviewed inventory must contain an `images` list with each image's `reference` and, when different, `archive_reference`. The check runs only local version/image-list commands and fails if the runtime sandbox pin differs from kubeadm or a required core image is absent.

