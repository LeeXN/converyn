# 交互式 Shell 安装器

[English](../../en/reference/install-script.md) | [简体中文](install-script.md)

入口是 [scripts/ops/install.sh](../../../scripts/ops/install.sh)。运行时只使用 Bash、系统命令和 Converyn 二进制，无 Python、jq、Rust 或 Node.js 依赖。适用于全新安装，以及恢复同一安装记录。安装器可在一台操作机上通过已验证 SSH 部署 1/3/5 个控制器并接入多个 Agent，也可在单机交互完成。

## 统一的 Shell 接口

两个 Shell 入口共用 `--binary`、`--installation`、`--yes`、`--non-interactive` 和 `--help`。安装记录创建和后续读取都使用 `--installation`；`init.sh` 的新交付目录使用 `--output`。SSH 选择器统一为 `--ssh-host`、`--ssh-user`、`--ssh-key`、`--ssh-port`（22）与 `--ssh-known-hosts`（`~/.ssh/known_hosts`）；deploy 通过拓扑参数选择多个目标，不接受单目标 `--ssh-host`。底层 `converyn admin` CLI 仍使用其原生参数名。

无参数且有终端时进入向导，显式动作也可补齐缺失的必填输入。`--yes` 接受计划；`--non-interactive` 禁止提问，无人值守执行需同时使用两者。不适用于当前动作的参数会被拒绝。二进制依次检查显式路径、PATH、同目录发行文件与仓库构建产物；显式路径无效时直接报错。

进度、诊断和底层命令输出写 stderr；成功后 stdout 仅输出一份文本摘要，包含 `Action`、`Result`、相关记录/交付/配置路径和后续命令。失败时 stdout 为空。退出码：成功或帮助为 0，缺失或非法输入为 2，执行失败或取消为 1。取消发生在持久化安装记录、身份签发及主机安装之前。既有私有记录保留原格式和校验约束，恢复仍需原冻结安装器与二进制。


## 交互安装

```bash
sudo bash scripts/ops/install.sh --binary ./converyn-linux-amd64
```
选择本机一体、单控制器加多 Agent、HA、仅 Agent 或仅 CLI。安装向导询问固定 IP、端口、Node UID、SSH 目标和私有记录目录，显示计划后执行。每台服务主机需要 Linux、systemd、Bash 4.4+、GNU coreutils、flock 和 root 权限；操作机需 curl，SSH 目标还需 OpenSSH/scp/tar。控制器自动配置 firewalld 时还需 `firewall-cmd` 和 `ip`。远端非 root 账号需要 sudo 权限；交互部署时，没有免密 sudo 的账号可以按提示输入 sudo 密码。

使用完整发行二进制，并保证操作机和所有目标主机架构一致。发行目录同时提供 `install.sh`，脚本会识别同目录匹配架构的 `converyn-linux-amd64` 或 `converyn-linux-arm64`；也可明确指定文件。首次运行没有二进制时，可交互输入 HTTPS 下载地址和独立审核的 SHA-256。脚本不自行选择 latest，不关闭 TLS/SSH 验证。控制器防火墙端口按下节规则自动准备。二进制必须满足当前安装器的配置包契约；仅凭相同版本号无法证明构建匹配。

## 向导缓存与编辑

集群名、管理员名、SSH 用户名和 Agent UID 没有内置身份默认值；需要明确输入，或复用本人此前保存的缓存。非交互部署必须提供 `--operator-name`，自动推导 SSH 控制器目标时还需 `--ssh-user`。文档示例中的名称和路径仅为示例。

每个控制器必须填写非空、有效的 IPv4/IPv6 地址及 1–65535 的端口。先在 IP 一项填写控制器固定地址；后面的安装目标默认是 `SSH用户@该IP`，回车即可。向导里直接填 IP 或主机名会自动补上所选 SSH 用户；命令行 `--controller`/`--agent` 仍使用 `local` 或 `user@host`。空 IP、格式错误、控制器 IP/主机重复、Agent UID/主机重复都会当场提示并留在当前项，不必从头输入。一个控制器和一个 Agent 仍可共机。

通过校验的非敏感选项会立即保存到 `${XDG_STATE_HOME:-$HOME/.local/state}/converyn/installer/inputs.tsv`，权限为 0600。下次启动时直接预填，回车接受，需修改时使用方向键和 Backspace；BS、DEL 两种退格编码均支持，Ctrl+U 可清空当前项，必填项不能留空。安装器自己的 sudo 密码隐藏输入也支持两种 Backspace。密码、令牌、下载 URL 和安装确认不会缓存；缓存只按白名单数据读取，不会作为 Shell 脚本执行。

可用 `--input-cache /绝对路径/inputs.tsv` 选择另一套缓存，或 `--no-input-cache` 禁用读写。显式命令行参数优先于缓存。缓存属于当前执行账号，用 sudo 运行时使用 root 的缓存。安装记录目录默认在当前运行账号的 `$HOME/.local/share/converyn-install/集群名`；目录不可写或已有安装记录会在输入这一项时提示。

输入缓存与安装记录分开：中断、输入校验失败后保留已确认选项，但不会自动启用 `--resume`、跳过确认或替换已有凭据/状态。恢复安装仍需保留原记录目录及冻结的安装器/二进制，并明确加 `--resume`。删除输入缓存文件只清除向导默认值。

## 主机防火墙与 SELinux

控制器安装默认使用 `--firewall auto`，从控制器配置读取实际监听地址/端口。firewalld 运行时，在监听接口所属 zone 同时添加运行时及永久 TCP 端口规则，不 reload 其他规则；UFW 运行时添加该 TCP 端口规则。仅监听回环地址的控制器无需入站规则，Agent 也无需 Converyn 入站端口。其他防火墙策略由管理员管理；由外部统一维护规则时可用 `--firewall skip`。

服务启动后，安装器检查操作机及所有目标主机到每个控制器的 TCP 连接，再确认 Leader 和多数派就绪，随后才创建首个人员凭据。网络失败会列出来源和目标，保留原记录，并在签发 OP 前停止。已有 `operator/operation.json` 会恢复原请求，不重新签发身份。

SELinux 默认 `--selinux keep`。显式传入 `--selinux permissive` 时，只在当前为 Enforcing 时执行 `setenforce 0`，仅本次启动有效；不会写入永久 disabled 或重启主机。防火墙/SELinux 选项会在安装计划中显示，并传给远端主机安装过程。

## 使用密码远程部署

交互部署无需提前配置 SSH 免密或 sudo 免密。在终端运行向导并选择 `password`，或明确传入 `--ssh-auth password`：

```bash
bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation "$HOME/.local/share/converyn-install/qa" \
  --member 1=192.0.2.11:8443 --member 2=192.0.2.12:8443 --member 3=192.0.2.13:8443 \
  --controller 1=operator@192.0.2.11 --controller 2=operator@192.0.2.12 \
  --controller 3=operator@192.0.2.13 --agent node-a=operator@192.0.2.21 \
  --ssh-auth password --ssh-known-hosts "$HOME/.ssh/known_hosts"
```

OpenSSH 提示输入各目标的 SSH 登录密码。该账号需要 sudo 密码时，安装器另行无回显询问，两种密码可以不同。SSH/scp 通过本次运行的私有套接字复用连接；sudo 密码只保留在当前进程内存中，通过加密 SSH 的标准输入传递，特权安装程序的标准输入不含密码。密码不接受命令行明文值，不导出到环境变量，也不写入安装记录、远端暂存或日志。退出时关闭本次 SSH 主连接并删除套接字；连接过期后可能重新认证。恢复安装时重新询问密码，继续复用原集群身份。

`--ssh-auth auto` 是默认值，支持密钥/ssh-agent，并在交互模式下回退到密码。`--ssh-auth key` 要求可用的密钥/ssh-agent，交互时仍可输入 sudo 密码。`--ssh-auth password` 选择 SSH 密码/键盘交互认证，不可与 `--ssh-key` 同用；需要终端，不可加 `--non-interactive`。`--yes` 仅确认安装计划，仍可询问认证密码。完全非交互部署继续要求 SSH 密钥/agent 和免密 sudo。

请选择可写的 `--ssh-known-hosts` 文件。首次连接时，交互式 OpenSSH 会显示主机指纹；与可信来源核对后输入 `yes`，指纹就会写入该文件。文件不存在时会以 0600 创建；已记录指纹发生变化时会拒绝连接。非交互运行必须提前在已有文件中记录已验证的指纹，不能弹出确认。只需要操作机能够 SSH 登录目标；控制器、Agent 正常运行不需要彼此 SSH 登录。如果操作机也安装服务，仍需本机 sudo。安装器不会替目标主机修改 SSH 或 sudo 策略。

## 非交互部署

本机一体安装：

```bash
sudo bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation /secure/converyn-install/qa \
  --member 1=127.0.0.1:8443 --controller 1=local --agent node-a=local \
  --yes --non-interactive
```
回环地址只适用于同机访问；需要外部 Agent、CLI 或浏览器时，改用所有客户端可达的固定 IP。

三控制器和两个 Agent：

```bash
bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation "$HOME/.local/share/converyn-install/qa" \
  --member 1=192.0.2.11:8443 --member 2=192.0.2.12:8443 --member 3=192.0.2.13:8443 \
  --controller 1=operator@192.0.2.11 --controller 2=operator@192.0.2.12 \
  --controller 3=operator@192.0.2.13 \
  --agent node-a=operator@192.0.2.21 --agent node-b=operator@192.0.2.22 \
  --ssh-key "$HOME/.ssh/id_ed25519" --ssh-known-hosts "$HOME/.ssh/known_hosts" \
  --yes --non-interactive
```
本例为非交互部署，SSH 主机密钥必须提前独立核验并记录在 `known_hosts`。每台主机最多一个控制器和一个 Agent，可以共机；多个控制器不能共用一台主机。只向控制器发送自己的 `controller-N` 目录，不发送离线 CA/个人签发机构或 OP 私钥；向 Agent 发送自己的短期一次性引导包。

安装过程：校验并固定二进制与脚本 → 生成私有配置包 → 目标预检 → 安装 systemd 服务 → 初始化原成员表 → 创建具名超级管理员 → 对每个 HTTPS 入口执行 mTLS 业务读取 → 安装 Agent 并核对 30 秒内的新鲜认证心跳。`systemctl active` 本身不算安装成功。

## 分步准备与节点安装

```bash
bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation "$HOME/.local/share/converyn-install/qa" \
  --member 1=192.0.2.11:8443 --member 2=192.0.2.12:8443 --member 3=192.0.2.13:8443 \
  --ssh-user operator --prepare-only --yes --non-interactive
```
`--prepare-only` 仅生成配置、签发材料和安装记录，不连接 SSH、不安装服务。随后使用相同集群、成员、目标、Agent UID、二进制和策略参数，加 `--resume` 执行；SSH 认证模式、密钥、端口和 known-hosts 选择器可以明确更新；交互密码会重新询问。也可私下把对应控制器目录和二进制送到目标机执行：

```bash
sudo bash install.sh controller --binary ./converyn-linux-amd64 \
  --bundle ./controller-2 --yes --non-interactive
```
为新增 Agent 创建一次性引导包，私下送到该节点，再在节点执行第二条命令：

```bash
converyn --context qa-op admin bootstrap --node node-c --output /secure/node-c-bootstrap.json
sudo bash scripts/ops/install.sh agent --binary ./converyn-linux-amd64 \
  --bootstrap /secure/node-c-bootstrap.json --yes --non-interactive
```
已加入的 Agent 信任控制面批准的流程，无需节点权限文件；主机脚本声明 `execution: host`。

只安装 CLI，也可用明确的 HTTPS 单文件地址；替换下面的摘要占位：

```bash
sudo bash scripts/ops/install.sh client \
  --binary-url https://release.example.test/converyn-linux-amd64 \
  --sha256 REVIEWED_64_HEX_SHA256 --yes --non-interactive
```
## 安装完成后

完整 `deploy` 会把已创建的个人 OP 导入操作用户的默认 CLI 配置，context 名称使用本次输入的集群名。目标文件优先采用 `CONVERYN_CONFIG`，未设置时为 `~/.converyn/config.json`；通过 sudo 启动时，使用调用 sudo 的普通用户的 home 和文件归属。首次导入会选为当前 context，已有选择保持不变。同一凭据重复导入可安全复用；同名 context 指向不同凭据时会保留并提示，不会直接覆盖。可用 `--no-default-context` 跳过这一步本机交付。

```bash
converyn status
converyn get nodes
converyn get workflows
converyn get runs
converyn config get-contexts
```

多个 context 使用 `converyn config use-context 你的context` 切换。`converyn agent --config PATH` 用来启动本机 Agent 进程，查看节点用 `converyn get nodes`。纯远程部署时，如果操作机本身不是服务目标，不会额外安装 PATH 中的 CLI；需安装同一二进制，或直接使用安装记录中的快照。

此前已完成的安装，只需以日常执行 CLI 的账号导入现有 OP 一次（替换为实际路径和本地别名）：

```bash
converyn config import /path/to/installation/operator/config.json --name YOUR_CONTEXT
converyn config get-contexts
```

源文件须对该账号可读。root 管理的安装记录继续保持私有，不要把整个记录目录改为其他人可读。也始终可用 `converyn --config /path/to/installation/operator/config.json status` 显式指定原配置。

复用安装器已创建的 OP，不重新签发。新增人员、自定义 context 文件、节点和浏览器材料见[初始化脚本](init-script.md)，首次节点组/流程登记及执行见[完整快速流程](../tutorials/quickstart.md)。`--prepare-only` 尚未启动控制器、初始化成员或创建 OP，需先以相同计划加 `--resume` 完成安装。

## 恢复与安装记录

```bash
sudo bash /secure/converyn-install/qa/install.sh deploy \
  --binary /secure/converyn-install/qa/converyn \
  --cluster qa --operator-name OP --installation /secure/converyn-install/qa \
  --member 1=127.0.0.1:8443 --controller 1=local --agent node-a=local \
  --resume --yes --non-interactive
```
安装目录保留冻结的 `install.sh`、`converyn`、`plan.tsv`、原 CA/密钥环、个人 OP 配置、节点引导包和检查点。恢复复用这些身份；脚本/二进制版本变化会拒绝混用，按示例执行保存的副本。已完成 Agent 接入时只核对新鲜心跳，不反复安装已有监督器。SSH 临时目录丢失时可用原配置安全重新暂存。

生成目录为 0700，凭据和诊断为 0600。不要提交安装目录，也不要将整个目录发给普通节点。引导包默认一小时有效，过期、部分签发材料、现场配置已改或未知执行结果需要先核查，脚本会停止并保留证据，不删除状态、重新生成 CA 或自动恢复其他 Node 身份。已有实例升级按[运维指南](../how-to/maintenance.md)处理。手工安装细节见[安装指南](../how-to/installation.md)。

## 验证

```bash
bash -n scripts/ops/install.sh
python3 -m unittest discover -s scripts/tests/install -p 'test_*.py' -v
env CONVERYN_INSTALL_LIVE=1 python3 -m unittest discover \
  -s scripts/tests/install -p 'test_*.py' -v
```
维护者夹具在临时目录使用真实单/三控制器、个人 mTLS 身份及 Agent；替换 systemd/SSH 安装动作以避免改动宿主。它验证配置包、成员初始化、逐入口业务读取、心跳和恢复，不冒充已在生产主机安装服务。见[回归说明](../development/testing.md)。
