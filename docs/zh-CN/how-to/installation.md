# 安装与首次运行

[English](../../en/how-to/installation.md) | [简体中文](installation.md)

本指南覆盖单机、单控制器加多 Agent、3/5 成员 HA。所有命令针对**全新实例**；已有环境按[运维指南](maintenance.md)备份与升级。以下 `/secure` 是操作机上的私有管理目录；将示例 IP、用户名、路径替换成实际值。

mTLS 即双向 TLS：连接仍使用 HTTPS 加密传输，客户端验证控制器证书，控制器也验证 CLI/Agent 的客户端证书。令牌用于绑定具体身份，步骤授权限定允许执行的工作；三者共同作用。无需额外桥接端口或传输服务。

推荐先按[从安装到首次部署](../tutorials/quickstart.md)使用 `install.sh` 和 `init.sh` 完成安装、个人身份及节点接入。脚本完整参数分别见[安装器](../reference/install-script.md)和[初始化脚本](../reference/init-script.md)。以下是逐步手工安装参考；已由安装器完成的配置生成、成员初始化和首个 OP 创建无需重复。

## 1. 准备主机和二进制

- 控制器和 Agent 运行于 Linux。标准 Agent 安装需要 systemd、`/bin/bash`、root 权限和 `useradd`；安装器创建独立的 `converyn-task` 低权限执行账号。控制器可用独立服务账号运行。
- 操作机使用[主 README 的构建步骤](../../../README.zh-CN.md)准备 `converyn`。复制到其他机器时使用相同构建、匹配架构的二进制。`converyn get plugins --local` 检查实际插件。
- 为每个控制器分配稳定的 IP 和本地持久磁盘，校准主机时间。初始化参数是 `编号=IP:端口`，不是 DNS 名或通配地址；所填 IP 同时用于监听、访问和证书 SAN。

| 网络方向 | 示例端口 | 用途 |
| --- | --- | --- |
| 操作机/浏览器 → 控制器 | TCP 8443 | CLI、GUI、API，使用 mTLS |
| Agent → 每个控制器 | TCP 8443 | 登记、拉取、领取、心跳及回报 |
| 控制器 ↔ 控制器 | TCP 8443 | Raft 和内部请求，与 API 共端口 |
| 操作机 → Agent 主机（可选） | TCP 22 | SSH 安装；普通执行不依赖 SSH |

Agent 无需入站执行端口。制品源、软件源和插件目标的访问按流程另外配置。每套环境使用独立名称、CA 和状态目录。

## 2. 选择一种拓扑并生成配置

在操作机执行。输出目录必须不存在，不要针对同一环境重复生成 CA。

本文手工 `admin init` 的输出根目录是 `/secure/converyn-qa`，直接包含 `admin/`、`user-authority/` 和 `controller-N/`。安装器记录根目录 `/secure/converyn-install/qa` 将这些内容放在 `bundle/` 下，另有 `operator/` 与检查点。`init.sh --installation` 只接受安装器记录根目录；手工配置使用显式 `--admin-config`、`--authority`，见快速流程的手工安装章节。

**单机：** 控制器和 Agent 都在本机，可使用回环地址：

```bash
install -d -m 0700 /secure
converyn admin init --directory /secure/converyn-qa --cluster qa \
  --member 1=127.0.0.1:8443
```

**单控制器 + 多 Agent：** 将上面的地址改为控制器实际内网 IP，例如 `1=192.0.2.11:8443`。远端 Agent 无法使用操作机或控制器的回环地址。

**HA：** 一次生成 3 个成员，每个控制器部署到独立主机。5 成员按同样方式追加 `--member 4=... --member 5=...`。

```bash
install -d -m 0700 /secure
converyn admin init --directory /secure/converyn-qa --cluster qa \
  --member 1=192.0.2.11:8443 \
  --member 2=192.0.2.12:8443 \
  --member 3=192.0.2.13:8443
```

支持固定的 1/3/5 成员拓扑；`replace members` 用于替换成员，不能将已运行的单成员在线扩成三成员。3/5 成员分别需要 2/3 个可用成员才能继续提交。

| 生成内容 | 放置与职责 |
| --- | --- |
| `controller-N/` | 只给对应控制器，含 `server.json`、TLS、peer 认证与在线 Node 签发材料 |
| `members.json` | 操作机保留，初始化时使用 |
| `admin/config.json` | 引导凭据，仅用于首个具名管理员及规定的恢复 |
| `user-authority/` | OP 操作机离线保护，用于签发个人身份 |
| `authority/`、`node-authority/` | 保护并备份的 CA 材料，不分发给普通用户或 Agent |

`server.json` 内相对路径以它所在目录解析，默认 `data_dir` 为 `data`。每个成员使用自己的数据库和目录，不能共享 SQLite。不要把整个配置包复制给每台工作节点。

## 3. 安装并启动控制器服务

本机直接使用生成的 `controller-1` 目录。远程 HA 先通过已验证主机密钥的 SSH 分发**对应目录和二进制**；例如：

```bash
scp -rp /secure/converyn-qa/controller-1 operator@192.0.2.11:~/
scp -p target/release/converyn operator@192.0.2.11:~/converyn
```

在目标控制器执行一次以下步骤；远端 `BUNDLE` 改为 `$HOME/controller-1`，编号 2/3 的机器使用各自的目录。安装二进制可用 `sudo install -m 0755 ~/converyn /usr/local/bin/converyn`。

```bash
BUNDLE=/secure/converyn-qa/controller-1
sudo useradd --system --user-group --no-create-home --shell /usr/sbin/nologin converyn-controller
sudo install -d -m 0700 /var/lib/converyn-controller
sudo cp -a "$BUNDLE/." /var/lib/converyn-controller/
sudo chown -R converyn-controller:converyn-controller /var/lib/converyn-controller
sudo chmod 0700 /var/lib/converyn-controller
sudo tee /etc/systemd/system/converyn-controller.service >/dev/null <<'UNIT'
[Unit]
Description=Converyn controller
Wants=network-online.target
After=network-online.target
[Service]
User=converyn-controller
Group=converyn-controller
WorkingDirectory=/var/lib/converyn-controller
ExecStart=/usr/local/bin/converyn server --config /var/lib/converyn-controller/server.json
Restart=on-failure
RestartSec=2
UMask=0077
[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
sudo systemctl enable --now converyn-controller
sudo journalctl -u converyn-controller -n 30 --no-pager
```

此例假设服务账号尚不存在；已有账号先确认用途和权限。非 systemd 试用也可前台运行 `converyn server --config /secure/converyn-qa/controller-1/server.json`，与托管服务二选一。初始化之前没有 Leader，就绪检查可能不通过。

## 4. 初始化一次并创建个人身份

全部控制器进程启动后，在原操作机执行一次：

```bash
converyn admin initialize --config /secure/converyn-qa/admin/config.json \
  --members /secure/converyn-qa/members.json
converyn create identities --config /secure/converyn-qa/admin/config.json \
  --authority /secure/converyn-qa/user-authority --name OP --super-admin \
  --output /secure/qa-op
converyn config import /secure/qa-op/config.json --name qa-op --use
converyn --context qa-op create identities \
  --authority /secure/converyn-qa/user-authority --name Alice \
  --output /secure/qa-alice
converyn --context qa-op auth grant alice-demo --person Alice --role executor --workflow demo --preset qa
converyn config import /secure/qa-alice/config.json --name qa-alice
converyn --context qa-op status
converyn --context qa-op get members
```

后续管理使用具名 OP，不使用引导 admin。个人配置包含私钥和令牌，只把 Alice 的个人文件通过私有通道交给 Alice。默认配置目录 0700、文件 0600；轮换、备份签发材料和浏览器登录见[证书](certificates.md)与[GUI](web-console.md)。

API 配置优先级是 `--config` → `CONVERYN_CONFIG` → `~/.converyn/config.json`；`--context` 只选择该文件内的 context。首次导入自动选中，之后保留当前选择，`--use` 显式切换。同名替换需 `--replace`。`server`、`agent`、`admin renew` 使用独立的显式服务配置。

## 5. 安装 Agent

在操作机为每个节点创建一次性引导包，默认 900 秒有效：

```bash
converyn --context qa-op admin bootstrap --node node-a \
  --output /secure/node-a-bootstrap.json
```

**同机安装：** 在目标节点以 root 安装。为了接下来运行示例，无需准备节点权限文件：

```bash
sudo /usr/local/bin/converyn admin install-local --bootstrap /secure/node-a-bootstrap.json

```

已加入的 Agent 信任控制面批准的流程，无需节点权限文件；主机脚本声明 `execution: host`。

**远程安装：** 先校验并记录 SSH 主机密钥。此命令使用密钥或 SSH agent，目标账号需要无交互的 sudo：

```bash
converyn admin install-ssh --host 192.0.2.21 --user operator \
  --identity-file /secure/ssh-key --known-hosts /secure/known_hosts \
  --binary target/release/converyn --bootstrap /secure/node-a-bootstrap.json
```

已加入的 Agent 信任控制面批准的流程，无需节点权限文件；主机脚本声明 `execution: host`。

在节点用 `sudo converyn admin agent-info --state-dir /var/lib/converyn/state` 检查本地确认状态，在操作机用 `converyn --context qa-op get nodes node-a` 检查新鲜心跳。

## 6. 执行示例并验收

修改 `example/workflow/inventory.yaml` 中的已登记 UID、地址和主机名，再从仓库根目录执行：

```bash
converyn validate workflows --file example/workflow/workflow.yaml
converyn --context qa-op register workflows --file example/workflow/workflow.yaml
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --dry-run
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --wait
converyn --context qa-alice get runs
```

用提交返回的 `RUN_ID` 查询 `get runs RUN_ID`，确认文件和脚本两个步骤成功且有实际回执，`logs runs RUN_ID --step verify --node node-a` 应显示版本、节点端口和区域。在 Agent 上核对 `/opt/converyn-demo/config` 的实际内容。`--wait` 超时只停止等待，任务仍在执行，继续查询原 Run。

HA 还需逐个入口使用相同身份检查业务读取、成员与 Leader 一致性，并在可丢弃的验收流程中停掉一个控制器，确认多数派及原任务恢复，不能仅凭服务 `active` 判定 HA。Follower 本机就绪可能返回 503，诊断方法见[运维](maintenance.md)。

接下来阅读[使用指南](runs.md)与[流程和脚本编写](workflows.md)。
