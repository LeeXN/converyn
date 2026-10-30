# 安装 Kubernetes 与基础组件

[English](README.md) | [简体中文](README.zh-CN.md)

在一个 Converyn Run 中安装 containerd、kubeadm Kubernetes、Calico、NFS CSI、cert-manager、Traefik、Harbor 和监控。支持一或三个控制节点及工作节点。使用空白 Ubuntu 24.04 或已准备的 Rocky Linux 8/amd64 节点，每台至少 4 GiB 内存和 30 GiB 可用磁盘；跨节点验证需要两个工作节点。Converyn 控制器与 Kubernetes 控制节点的角色分别配置。

## 阅读流程

| 文件 | 用途 |
| --- | --- |
| [workflow.yaml](workflow.yaml) | 原生有序阶段，在准备后的目录中直接使用 |
| [cluster.yaml](cluster.yaml) / [cluster-ha.yaml](cluster-ha.yaml) | 单控制节点/HA 环境输入；替换 UID、地址及服务配置 |
| [stages.yaml](stages.yaml) | 准备配方：模板文件、kubeadm 动作及阶段输入 |
| [sources.lock.json](sources.lock.json) | 审核过的组件版本、来源和校验摘要 |
| [configs/](configs) / [scripts/](scripts) | 主机/Helm 配置与实际部署、验证脚本 |

顺序为预检 → 运行时 → 首个控制节点 → 串行加入其他控制节点 → 工作节点 → 网络 → 存储 → 证书 → 入口 → 镜像仓库 → 监控 → 功能验证。所有节点完成当前阶段后才进入下一阶段。默认 NodePort 为 HTTP 30080、HTTPS 30443。

## 准备固定输入

使用 `full` 二进制和具名 OP。完成 [Converyn 安装](../../docs/zh-CN/tutorials/quickstart.md)及节点登记，再从仓库根目录执行。下面使用单控制节点清单；HA 改复制 `cluster-ha.yaml`。输入变化时使用新输出目录。

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

本地工具下载、校验制品并准备私有 kubeadm 材料，不会在节点执行安装。[准备细节](PREPARATION.zh-CN.md)涵盖 Rocky 内核/cgroup 条件、主机初始化、HA 路由、已有 NFS、托管制品分发、节点组和私有文件准备。**部署前必须完成这些步骤。** CA/私钥、引导令牌和密码文件留在私有位置；密码配置填写路径，不填密码值。

准备后得到 `.local/kubernetes-deployment/workflow/`，包含 `workflow.yaml`、`inventory.yaml`、`artifacts.yaml` 及可读的 `prepared/` 脚本和配置。这是普通原生源目录，登记时解析文件摘要并打包定义。私有 kubeadm 材料在同级目录，不会进入登记内容。先用 node-group.json 创建平台节点组，再登记流程；Python 只用于准备源文件。

## 部署

已登记节点按流程执行主机操作，无需节点授权片段。

```bash
converyn --context qa-op update node-groups --file .local/kubernetes-deployment/node-group.json
converyn validate workflows --file .local/kubernetes-deployment/workflow
converyn --context qa-op register workflows --file .local/kubernetes-deployment/workflow
workflow_id=$(jq -r '.[0].workflow_uid' .local/kubernetes-deployment/sequence.json)
converyn --context qa-op run workflows "$workflow_id" --target default --dry-run
converyn --context qa-op run workflows "$workflow_id" --target default \
  --request-id kubernetes-install-001 --wait --timeout-seconds 7200
```

流程所有者根据环境输入保持稳定。预览检查计划工作；实际执行检查授权、来源、文件和目标状态。网络重试保留请求 ID，结果不明时查询原始 Run。

## 验证与恢复

```bash
converyn --context qa-op get runs run-kubernetes-install-001
converyn --context qa-op logs runs run-kubernetes-install-001 --follow
```

最后阶段验证节点角色、DNS、跨工作节点 HTTP、网络策略、持久存储、真实指标和 TLS OCI 推拉。证据保留在首个控制节点的 `/opt/converyn-example/evidence`。检查成功回执及业务结果，不能只看服务状态。

修复已安装集群时，不要重跑完整初始化和加入流程。检查原始 Run，保留私有材料、Helm 历史及 PVC，只准备受影响的恢复步骤。加入令牌两小时后过期，更新材料后须重新审阅再登记。[卸载流程](UNINSTALL.zh-CN.md)使用原安装目录，保留 NFS 数据和 Converyn 状态。

高级镜像归档、已有 Cluster 快照及配置编辑器见[操作工具](../../docs/zh-CN/how-to/kubernetes-tools.md)。本例是使用已有 NFS、私有 CA 和 NodePort 的实验环境配置。向已有集群发布应用使用[外部 Kubernetes](../external-kubernetes/README.zh-CN.md)。

默认清单使用独立的平台节点组，避免覆盖快速开始中的演示组。若通过 `--node-group-snapshot` 复用已批准的现有组，跳过创建组的命令，确认原组已包含全部目标节点。
