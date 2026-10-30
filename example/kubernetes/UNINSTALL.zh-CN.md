# 卸载 Kubernetes 并保留数据

[English](UNINSTALL.md) | [简体中文](UNINSTALL.zh-CN.md)

此操作移除本示例安装的集群。先停止该集群上的业务发布和持续核对，保留原安装生成目录；工具绑定原节点、CA、API 地址、归属和 kubeadm 摘要，不能用于接管其他集群。生成与登记不执行卸载，只有最后的运行命令会修改主机。

## 生成、预览与执行

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

## 顺序与保留边界

先在所有节点核验身份并保存 root 私有的 Kubernetes 配置及存储清单，再依次处理 worker、次级控制节点、首控制节点。每步验证 kubelet 挂载、容器任务、静态清单、etcd 成员和端口确已清理；失败时阻止后续步骤。

保留 NFS/PV 实际数据、NFS 服务、containerd 及镜像缓存、API 代理、原私有安装材料，以及 Converyn 凭据、服务、状态和历史。不调用 Helm uninstall 或删除 PVC/PV，不全清 iptables/IPVS，不删除其他 CNI。配置归档不是 etcd 灾备快照。

## 验收与重新安装

检查每台主机回执，以及 Converyn 认证心跳、NFS 挂载和原数据。结果未知时查询原 Run，网络重试沿用请求 ID；新一轮卸载必须用新的请求 ID。重新安装使用新生成目录和新的 kubeadm 材料；旧 NFS 数据保留，不自动接管原 PVC。参考 [kubeadm reset](https://kubernetes.io/docs/reference/setup-tools/kubeadm/kubeadm-reset/) 和[安装指南](README.zh-CN.md)。

## 从已备份的中断位置恢复

先核查原 Run 的失败步骤及保存的 reset 日志。若所有检查和备份已完成，使用原安装材料、实际失败 Node UID 和新输出目录继续；已完成节点不会再次 reset。仍有本机配置时按原 CA 校验后执行 reset；配置已移除时只接受原 root 私有备份，核验硬件、CA、空静态清单和关闭的控制面端口后完成清理。发现新集群、证据缺失或剩余 etcd 成员不符会停止。

```bash
.venv/bin/python scripts/ops/kubernetes/render_uninstall.py \
  --installation .local/kubernetes-deployment --converyn /path/to/current/converyn \
  --resume-from control-3 --output .local/kubernetes-uninstall-resume
```

登记、预览并以新的请求 ID 运行生成的 `workflow/`。流程先将 Calico 的清理连接固定到原 API 入口，并用固定版本 crictl 删除依赖 CNI 的 sandbox；API/etcd 等 host-network sandbox 留给后续 kubeadm reset，避免最后一个 API 停止后仍需调用 CNI。不要删除备份来绕过检查。
