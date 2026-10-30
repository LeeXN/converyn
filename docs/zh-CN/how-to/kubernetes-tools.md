# Kubernetes 操作机工具

[English](../../en/how-to/kubernetes-tools.md) | [简体中文](kubernetes-tools.md)

示例源文件位于 [example/kubernetes](../../../example/kubernetes/README.zh-CN.md)，运行用的 Shell/配置仍在该目录。本目录的 Python 只负责下载和准备固定输入，不是 Agent 上的业务脚本。回归验收统一在 [scripts/tests/kubernetes](../development/testing.md)。

使用 `converyn register workflows` 登记准备好的原生目录，再通过 `converyn run workflows` 执行；完整命令见示例。

## 操作机工具

在操作机上从仓库根目录运行。`prepare.py` 下载制品，`render.py` 准备原生流程目录和私有材料。这些工具不连接 Converyn 控制面。

### prepare.py：下载制品

读取 [sources.lock.json](../../../example/kubernetes/sources.lock.json)，通过 HTTPS 下载固定版本的二进制和 Chart，核对 SHA256，并提取清单指定的归档成员。依赖 Python 3 和 `curl`。

| 参数 | 必填 | 说明 |
| --- | --- | --- |
| `--output DIR` | 是 | 制品目录；输入相同时可复用 |
| `--images-archive FILE` | 否 | 操作员提供的 Docker/OCI tar 镜像归档，最大 10 GiB |

`DIR` 下生成：

| 路径 | 内容 |
| --- | --- |
| `downloads/` | 上游下载文件和提取的二进制 |
| `blobs/SHA256` | 供 HTTPS 制品源使用的内容寻址文件 |
| `artifacts.json` | 组件版本及各制品的目标路径、SHA256、长度、权限和来源摘要 |

摘要一致的下载文件和 blob 会被复用；输入变化时使用新输出目录。下载并发为 4，`curl` 重试次数为 3；校验失败时停止。可选镜像归档经格式检查、计算摘要后复制到 `blobs/`，在 runtime 阶段导入。

### render.py：生成部署文件

读取集群配置、制品清单、[stages.yaml](../../../example/kubernetes/stages.yaml)、[configs/](../../../example/kubernetes/configs) 和 [scripts/](../../../example/kubernetes/scripts)，调用本地 `converyn prepare plugins`、`expand plugins` 和 `compile plans` 命令。依赖 Python 3、[requirements.txt](../../../scripts/ops/kubernetes/requirements.txt) 和启用 `full` 的 Converyn 二进制。

| 参数 | 必填 | 说明 |
| --- | --- | --- |
| `--config FILE` | 是 | YAML 或 JSON 集群配置，可从 `cluster.yaml` 或 `cluster-ha.yaml` 开始 |
| `--assets FILE` | 是 | `prepare.py` 生成的 `artifacts.json` |
| `--converyn FILE` | 是 | Converyn 可执行文件路径 |
| `--output DIR` | 是 | 尚不存在的输出目录，以 0700 权限创建 |
| `--kubeadm-kit DIR` | 否 | addon 修复使用的原 `private-kubeadm/`；校验 CA、配置摘要、endpoint 和网络 |
| `--node-group-snapshot FILE` | 否 | 复用已登记 NodeGroup 作为范围；部署顺序仍来自 inventory |

| 输出 | 内容 |
| --- | --- |
| `workflow/` | 包含清单、准备后的文件/脚本源及参数绑定的原生目录；推荐登记入口 |
| `plans/STAGE.input.yaml`、`plans/STAGE.input.json` | 可供 `converyn compile plans` 离线编译的原生 PlanInput |
| `plans/STAGE.json` | 编译后的 Plan，包含 Work 和摘要 |
| `playbooks/STAGE.yaml` | 展开后的 Playbook |
| `definitions/SHA256.txt` | 固定的脚本和配置文本 |
| `workflows/STAGE.json` | 可交给 `register workflows` 的完整流程快照 |
| `node-group.json` | 平台节点组更新请求 |
| `private-kubeadm/` | CA、私钥、引导令牌和 init/join 配置 |
| `prepare-kubeadm.json`、`UID-expand.json`、`expanded-UID/` | 本地插件准备与展开的输入、输出 |
| `sequence.yaml`、`sequence.json` | 阶段顺序、workflow/request ID 和预览摘要 |
| `public-cluster.json` | 输入集群配置的副本 |

支持 1 或 3 个控制节点、至少 1 个工作节点，总计 2–20 个 Node；跨工作节点验收需要至少 2 个 worker。单控制节点包含 11 个子阶段，HA 包含 12 个子阶段，但均只生成一个安装 Workflow。生成目录按私有资料管理；节点文件分发位置见示例的[准备指南](../../../example/kubernetes/PREPARATION.zh-CN.md)。

`--node-group-snapshot` 绑定已批准节点组。两条路径都使用流程 inventory；平台成员顺序和额外成员不改变部署。

## 镜像清单校验与卸载生成

`check_image_inventory.py` 校验操作机的镜像归档清单、固定 kubeadm 摘要和 containerd sandbox 配置，只在本地运行版本与镜像列表命令，不检查远端安装结果。使用 `--kubeadm FILE --inventory FILE`，可选 `--containerd-config FILE`。

`render_uninstall.py --installation DIR --converyn FILE --output DIR [--resume-from NODE_UID]` 读取原安装输出并生成卸载流程，不登记或执行。边界与使用见[卸载示例](../../../example/kubernetes/UNINSTALL.zh-CN.md)。

依赖见 [requirements.txt](../../../scripts/ops/kubernetes/requirements.txt)；从仓库根目录运行 `python3 scripts/ops/kubernetes/工具名.py --help`。所有生成路径使用 `.local/` 等受保护目录。输入模板定位到仓库的 `example/kubernetes/`，不依赖当前工作目录；分发工具时需同时保留示例目录。
