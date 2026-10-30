# 从安装到首次部署

[English](../../en/tutorials/quickstart.md) | [简体中文](quickstart.md)

从全新实例开始，依次完成平台安装、个人证书/context、节点接入和示例执行。示例使用集群 `qa`、安装记录 `/secure/converyn-install/qa`。已有安装跳过部署，使用自己的记录目录和节点身份；已运行环境升级见[运维](../how-to/maintenance.md)。

## 1. 脚本放在哪里

在**操作机**保留 `install.sh`、[init.sh](../../../scripts/ops/init.sh) 和匹配的 Converyn 二进制。本文从仓库根目录运行，示例发行二进制为 `./converyn-linux-amd64`；源码构建时替换为 `./target/release/converyn`。在发行目录使用 `./install.sh` 和 `./init.sh`，流程示例仍需从仓库准备。两个脚本均不需要 Python/jq；安装机需要 Linux、systemd、Bash 4.4+、GNU coreutils 和 flock。

| 机器 | 放置内容 |
| --- | --- |
| 操作机 | 两个管理脚本、二进制、流程源码、私有安装记录/签发机构 |
| 控制器 | 安装器分发的本机 controller-N 配置，运行控制器服务 |
| Agent 节点 | 自己的节点交付目录；无需整个仓库、OP 配置或 CA 私钥 |

安装记录由 root 管理时，下面用 `sudo` 读取私有文件；这是文件权限，API 上的身份仍由所选 context 决定。普通用户可以把自己收到的配置导入自己的 `~/.converyn/config.json`。

## 2. 安装平台

交互安装时选择单机、单控制器多 Agent 或 HA，将私有记录目录设为 `/secure/converyn-install/qa`。要直接运行本文 demo，将 Agent UID 填为 `node-a`，并传入示例策略：

```bash
sudo bash scripts/ops/install.sh --binary ./converyn-linux-amd64

```

向导会生成配置、部署服务、初始化成员并创建具名 OP；选择接入 Agent 时还会核对认证心跳。完成后会自动导入操作用户的默认 OP context；业务流程与节点组仍需自行登记。HA/SSH 和准备后恢复的参数见[安装器说明](../reference/install-script.md)。

远程目标可在交互终端输入 SSH 和 sudo 密码：向导选择 `password`，或传入 `--ssh-auth password`，不加 `--non-interactive`。密钥方式继续支持，见[安装器认证说明](../reference/install-script.md#使用密码远程部署)。

向导会逐项保存已校验的非敏感输入，下次预填后回车复用，也可用方向键/Backspace 修改；密码和确认不缓存，见[输入缓存说明](../reference/install-script.md#向导缓存与编辑)。

若直接运行下面的单机示例，安装器同时为 node-a 配好最小流程的文件策略。该例所有访问都在本机，浏览器和远端 Agent 需要改成可达固定 IP：

```bash
sudo bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation /secure/converyn-install/qa \
  --member 1=127.0.0.1:8443 --controller 1=local --agent node-a=local \
  --yes --non-interactive
```

交互命令与非交互命令选择其一；不要对同一个实例重复执行全新部署。安装器只把二进制放到目标主机；纯远程部署后，若操作机尚无 `converyn` 命令，先安装同一构建：

```bash
sudo install -m 0755 ./converyn-linux-amd64 /usr/local/bin/converyn
```

使用安装器已生成的个人 OP 配置检查：

```bash
sudo converyn --config /secure/converyn-install/qa/operator/config.json status
sudo converyn --config /secure/converyn-install/qa/operator/config.json get members
```

## 3. 初始化个人证书和 context

交互向导：

```bash
sudo bash scripts/ops/init.sh --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json
```
安装器通常已自动导入默认配置；下例为示例单独建立 context 文件，复用已生成的 OP，不重新签发：

```bash
sudo bash scripts/ops/init.sh import --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json --context-name qa-op --use \
  --yes --non-interactive
sudo converyn --config /secure/converyn-install/qa/contexts.json config get-contexts
```
先准备凭据和节点输出的父目录；个人输出目录本身留给签发命令创建：

```bash
sudo install -d -m 0700 /secure/converyn-users /secure/converyn-nodes
```

为执行者和只读用户签发**独立个人证书**并创建 context：

```bash
sudo bash scripts/ops/init.sh user --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json \
  --name Alice --context-name qa-alice --role executor --workflow demo --preset qa \
  --output /secure/converyn-users/qa-alice --yes --non-interactive
sudo bash scripts/ops/init.sh user --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json \
  --name Reader --context-name qa-reader --role viewer --workflow demo \
  --output /secure/converyn-users/qa-reader --yes --non-interactive
```
context 是本地连接别名，权限由服务端角色绑定决定，不能通过改别名提升权限。上述 Alice 只允许执行 `demo/qa`，Reader 只允许查看 `demo`。

| role | 用途 |
| --- | --- |
| `viewer` | 查看授权流程及任务 |
| `executor` | 预览/执行授权流程，控制自己的任务 |
| `maintainer` | 维护、执行和处置指定流程，覆盖其全部预设 |
| `platform_operator` | 节点、节点组和控制面管理 |
| `access_admin` | 个人身份、用户组与权限管理 |
| `super_admin` | 全部实例管理能力 |

业务角色使用 `--workflow ID`，只读/执行者可加 `--preset NAME`；授予全部流程必须明确 `--all-workflows`。平台角色不自动拥有业务执行权限。个人角色是 API 权限，部署节点由流程 inventory 和 `hosts` 选择器确定。

## 4. 添加 Agent 节点

如果安装器已接入 node-a，跳过本节的签发和安装，直接核对目录及心跳。要添加尚未登记的 node-a，在**操作机**生成独立交付目录：

```bash
sudo bash scripts/ops/init.sh node --installation /secure/converyn-install/qa \
  --node node-a \
  --output /secure/converyn-nodes/node-a --yes --non-interactive
```
目录包含 `converyn`、`install.sh`、一次性 `bootstrap.json`、校验和及 `INSTALL.txt`；不含 OP 凭据或签发 CA。引导默认一小时有效。节点需与该二进制架构匹配。按生成的 `INSTALL.txt` 执行。节点信任控制面批准的操作，无需本机授权文件。`--ttl-seconds` 可指定 60–3600 秒。

**目标节点**的 SSH 用户先准备私有目录：

```bash
install -d -m 0700 "$HOME/converyn-node"
```
**操作机**通过已独立验证的 SSH 主机密钥复制目录；替换地址、用户名和 known-hosts 路径：

```bash
sudo scp -rp -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile=/secure/known_hosts \
  /secure/converyn-nodes/node-a/. operator@192.0.2.21:converyn-node/
```
在**目标节点**安装 Agent：

```bash
cd "$HOME/converyn-node"
sha256sum --check checksums.sha256
sudo bash ./install.sh agent --binary ./converyn --bootstrap ./bootstrap.json \
  --yes --non-interactive
```
回到**操作机**核对认证心跳：

```bash
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-op get nodes node-a
```
已加入的 Agent 信任控制面批准的流程，无需节点权限文件；主机脚本声明 `execution: host`。

已加入的 Agent 信任控制面批准的流程，无需节点权限文件；主机脚本声明 `execution: host`。

## 5. 业务脚本放在哪里，如何部署

业务脚本保留在**操作机的流程项目目录**。本例的 `example/workflow/` 包含 `workflow.yaml`、`inventory.yaml`、`verify.sh`、`config.txt`、`values.yaml`。`scripts`/`files` 路径相对于流程文件；登记时按摘要打包，Agent 领取授权后下载执行，不用手工把这些文件逐台复制到节点。

在 `inventory.yaml` 填入已登记 Node UID、地址和主机名，再从仓库根目录执行。先创建平台节点组，再由同时具备流程维护与平台运维权限的 OP 登记流程。`qa` 是用户自定义目标预设名称，见[节点组与流程范围](../how-to/node-groups.md)：

```bash
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-op \
  update node-groups --file example/node-group.yaml
converyn validate workflows --file example/workflow/workflow.yaml
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-op \
  register workflows --file example/workflow/workflow.yaml
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice \
  run workflows demo --target qa --set VERSION=two --request-id demo-001 --dry-run
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice \
  run workflows demo --target qa --set VERSION=two --request-id demo-001 --wait
```
查询实际回执和日志：

```bash
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice get runs run-demo-001
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice logs runs run-demo-001 --follow
```

## 6. 分发个人配置和浏览器证书

只把 `/secure/converyn-users/qa-alice/config.json` 私下交给 Alice，不能发送包含 OP 和其他人的整个 contexts 文件。Alice 将文件保存为 0600 后，在自己的电脑执行：

```bash
chmod 600 ./alice-config.json
converyn config import ./alice-config.json --name qa-alice --use
converyn --context qa-alice auth status
converyn --context qa-alice get workflows
```
操作机可为 Alice 导出浏览器交付材料，交互输入 P12 密码：

```bash
sudo bash scripts/ops/init.sh browser --file /secure/converyn-users/qa-alice/config.json \
  --output /secure/converyn-users/qa-alice-browser
```
非交互时添加 `--password-file /secure/private-password-file`，文件为 0600；输出为 `client.p12`、`token.txt`、CA 和登录说明。P12 导出需要操作机安装 OpenSSL。个人 PEM 证书与 P12 对应同一身份，角色通过权限绑定管理。

## 7. 后续扩容、恢复和手工安装

新增 node-b 时重复节点交付/安装，将实际 UID、地址和主机名追加到 `example/workflow/inventory.yaml`，再重新登记流程。登记 Agent 本身不会自动加入部署目标；已有 Run 保留原始清单。

证书签发或节点包生成中断后，用相同输入加 `--resume`，复用原请求；已有 context 默认拒绝覆盖，只有明确 `--replace-context` 才替换该别名。过期/已使用的节点引导或现有 Node 身份先核查，不通过换 UID 或删状态绕过。

手工安装且尚无具名管理员时，在控制器已经启动并完成 `admin initialize` 后，创建首个 OP：

```bash
bash scripts/ops/init.sh operator --admin-config /secure/converyn-qa/admin/config.json \
  --authority /secure/converyn-qa/user-authority \
  --name OP --context-name qa-op --output /secure/converyn-users/qa-op \
  --contexts /secure/converyn-qa/contexts.json --use --yes --non-interactive
```
详细安装见[安装指南](../how-to/installation.md)，权限与签发见[个人证书](../how-to/certificates.md)，动作、参数、恢复边界与常见错误见[初始化脚本说明](../reference/init-script.md)。浏览器导出不支持 `--resume` 覆盖目录；保留原包，需要重导出时使用新目录。
