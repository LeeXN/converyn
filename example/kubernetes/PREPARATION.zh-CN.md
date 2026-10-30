# Kubernetes 部署准备

[English](PREPARATION.md) | [简体中文](PREPARATION.zh-CN.md)

## 1. 准备操作机输入

操作机需要 Python、uv、curl、jq 和 full 二进制。先按[安装指南](../../docs/zh-CN/how-to/installation.md)准备控制面、具名 OP 和 Agent；不需要 publisher 或额外项目权限。下列 Python 工具只准备本地输入，不执行目标安装。

从仓库根目录使用[部署指南](README.zh-CN.md)中的命令准备一次 `.local/kubernetes-deployment/`。下面说明环境输入及需要分发的文件。

使用已安装 requirements 的 Python 环境，将文档地址 `192.0.2.0/24` 替换为目标地址。分别填写 Node UID、hostname 和 IPv4 地址；project、cluster_uid、owner_uid 使用稳定的示例命名与资源归属标识。首个 `control_plane: true` 节点执行 init，控制节点数量为 1 或 3。Pod、Service 和物理网络使用互不重叠的网段，并核对实际路由。

nfs.server/share 指向已准备的导出。harbor_host 是证书 SAN/Ingress 域名，NodePort 明确指定。密码字段填写 root 私有文件路径，不填密码；verification_image 固定到 digest。

每次 render 使用新输出目录。修复 addon 时，通过 `--kubeadm-kit 原目录/private-kubeadm` 复用原 CA 和配置，校验摘要、endpoint 和网络；恢复时单独编制受影响步骤的恢复 Plan，不要重跑完整 init/join。Join token 两小时过期；加入节点前刷新过期输入，审核摘要并更新登记的流程快照。

## 2. 准备主机、HA 路由及 NFS

### Rocky Linux 8 主机准备

Rocky Linux 8 的默认 4.18 内核和 cgroup v1 不满足本示例要求。先由操作员安装并启动审核过的 Linux 5.10 或更新内核，配置统一 cgroup v2 后重启；检查 `uname -r` 和 `/sys/fs/cgroup/cgroup.controllers`。门槛取自 [Calico 3.33 的内核要求](https://docs.tigera.io/calico/latest/getting-started/kubernetes/requirements)；[Kubernetes 的 cgroup v2 要求](https://kubernetes.io/docs/concepts/architecture/cgroups/)本身为 Linux 5.8 或更新。本例的 containerd 与 kubelet 均使用 systemd cgroup 驱动。

`bootstrap-node.sh` 在 Rocky 8 上使用 `dnf` 安装依赖，并让 NetworkManager 忽略 Calico 接口；它不会升级内核、修改引导参数、关闭防火墙或更改 SELinux 模式。隔离实验环境须明确准备 SELinux permissive 或 disabled，本例不提供 enforcing 模式的容器/Calico 策略。预检仍拒绝活动防火墙、swap、资源不足及已有 Kubernetes 状态。NFS 引导在 Rocky 上安装 `nfs-utils`，HA API 代理安装 HAProxy 并沿用同一配置校验和非接管检查。实际可用性以对应 VM 的安装回执和功能验收为准。

把该示例目录复制到各空白目标，以 root 执行：

```bash
sudo bash bootstrap-node.sh
```

脚本安装主机依赖、建立目录并持久关闭 swap。HA 样例在所有 Kubernetes 节点上、init/join 之前配置本地代理：

```bash
bash bootstrap-api-proxy.sh --print kube-ha.example.test 192.0.2.10 192.0.2.11 192.0.2.12
sudo bash bootstrap-api-proxy.sh kube-ha.example.test 192.0.2.10 192.0.2.11 192.0.2.12
```

替换域名和控制节点 IP。该名称映射本地回环代理的 16443 端口，再以 TLS 转发到三个 API Server；使用此入口的客户端节点也需本地代理。脚本拒绝接管无关 HAProxy 配置。也可自行提供独立管理的 HA 负载均衡入口。控制节点要分布到物理故障域，故障测试中保持客户端 endpoint 不变。

在单独或明确选定的 Ubuntu/Rocky NFS 主机上，使用新目录并列举全部 Kubernetes 客户端 IP：

```bash
sudo bash bootstrap-nfs.sh /srv/converyn-nfs NODE1_IP NODE2_IP NODE3_IP
```

使用新的 export 目录。实验 NFS 配置采用 no_root_squash 和 0777 权限；按部署环境配置存储身份、备份和可用性。在主机防火墙中放行实际 API、etcd、kubelet、Calico VXLAN UDP 4789、NFS TCP 2049 和所需 NodePort。

## 3. 节点接入与制品分发

已登记 Agent 执行批准后的流程，审核后的主机脚本声明 `execution: host`。无需修改 Agent 授权，也无需额外搭建 HTTPS 制品服务器。render 时将准备目录中的 `blobs/` 与 `artifacts.json` 放在一起；生成的原生流程将已校验字节复制到自己的 `assets/`，登记时上传至 Converyn 托管制品库。大文件不进入 Raft 日志，Agent 只下载当前 Run 授权的制品。

## 4. 配置私有 kubeadm 输入

通过私有通道传输；目录 root 所有、0700，文件 0600：

| 生成文件 | 节点路径 | 节点 |
| --- | --- | --- |
| private-kubeadm/ca.crt | /etc/converyn-example/kubeadm/ca.crt | 全部 |
| private-kubeadm/init.yaml | /etc/converyn-example/kubeadm/init.yaml | 首个控制节点 |
| private-kubeadm/join-control-plane.yaml | /etc/converyn-example/kubeadm/join-control-plane.yaml | 其他 HA 控制节点 |
| private-kubeadm/join-worker.yaml | /etc/converyn-example/kubeadm/join-worker.yaml | 工作节点 |
| private-kubeadm/control-plane-pki/ca.crt、ca.key | /etc/kubernetes/pki/ca.crt、ca.key | 仅首个控制节点 |

在首个控制节点生成两个独立随机密码，写入配置指定的 Harbor/Grafana 文件，root 所有、0600，单行 UTF-8 且无首尾空白。创建 Kubernetes Secret 时通过管道传递，不写入公共定义；管理员 kubeconfig、etcd、Helm 状态也须保护。

节点需要访问组件/Chart 的官方镜像仓库。受限网络可准备来自**相同官方仓库**的完整审核过的 Docker/OCI 归档，包含 Calico init、cert-manager、Operator 动态创建的监控镜像及验收镜像：

```bash
python3 scripts/ops/kubernetes/prepare.py --output .local/kubernetes-offline-assets \
  --images-archive /private/reviewed-images.tar
```

用新制品清单重新 render。归档按摘要和长度固定，分发到全部 Node，在 runtime 阶段导入。准备前完成镜像归档的构建与完整性检查，保留来源仓库 digest、镜像配置 ID 和归档摘要。

render 前，用已校验摘要的 kubeadm 下载件核对核心镜像清单和 containerd sandbox 配置：

```bash
python3 scripts/ops/kubernetes/check_image_inventory.py \
  --kubeadm .local/kubernetes-offline-assets/downloads/kubeadm \
  --inventory /private/images.json
```

审核过的清单使用 `images` 数组，每项包含 `reference`；归档引用不同时再提供 `archive_reference`。检查只运行本地版本和镜像列表命令；sandbox 配置与 kubeadm 不一致或归档缺少核心镜像时立即失败。

