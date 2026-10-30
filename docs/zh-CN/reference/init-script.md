# 个人身份与节点初始化

[English](../../en/reference/init-script.md) | [简体中文](init-script.md)

[init.sh](../../../scripts/ops/init.sh) 在操作机调用现有 CLI，完成个人证书签发、服务端授权、本地 context 导入、节点交付和浏览器凭据导出。平台部署使用 [install.sh](../../../scripts/ops/install.sh)；端到端命令见[从安装到首次部署](../tutorials/quickstart.md)。初始化需要连接正在运行的控制器。

## 统一的 Shell 接口

两个 Shell 入口共用 `--binary`、`--installation`、`--yes`、`--non-interactive` 和 `--help`。安装记录创建和后续读取都使用 `--installation`；`init.sh` 的新交付目录使用 `--output`。SSH 选择器统一为 `--ssh-host`、`--ssh-user`、`--ssh-key`、`--ssh-port`（22）与 `--ssh-known-hosts`（`~/.ssh/known_hosts`）；deploy 通过拓扑参数选择多个目标，不接受单目标 `--ssh-host`。底层 `converyn admin` CLI 仍使用其原生参数名。

无参数且有终端时进入向导，显式动作也可补齐缺失的必填输入。`--yes` 接受计划；`--non-interactive` 禁止提问，无人值守执行需同时使用两者。不适用于当前动作的参数会被拒绝。二进制依次检查显式路径、PATH、同目录发行文件与仓库构建产物；显式路径无效时直接报错。

进度、诊断和底层命令输出写 stderr；成功后 stdout 仅输出一份文本摘要，包含 `Action`、`Result`、相关记录/交付/配置路径和后续命令。失败时 stdout 为空。退出码：成功或帮助为 0，缺失或非法输入为 2，执行失败或取消为 1。取消发生在持久化安装记录、身份签发及主机安装之前。既有私有记录保留原格式和校验约束，恢复仍需原冻结安装器与二进制。


## 准备与交互入口

需要 Linux、Bash 4.4+、GNU coreutils、grep、sed、flock 和可执行的 Converyn 二进制；浏览器 P12 导出还需要 OpenSSL。无需 Python/jq。节点交付要求 `init.sh` 和 `install.sh` 位于同一目录。

```bash
bash scripts/ops/init.sh --help
bash scripts/ops/init.sh --binary ./converyn-linux-amd64 \
  --installation "$HOME/.local/share/converyn-install/qa"
```

不指定动作时打开向导。`--binary` 可显式选择二进制，否则先找 PATH 中的 `converyn`，再找脚本旁匹配架构的发行二进制、脚本旁的 `converyn`、仓库 `target/release/converyn`、`target/debug/converyn`。需要确保使用当前安装对应的构建时，显式指定 `--binary`。

安装记录属于 root 时用 `sudo`，并显式指定 `--contexts`，避免把个人配置意外导入 root 的默认目录。私有输入文件须为 0600，输出父目录须为当前执行用户所有、权限 0700。路径使用规范绝对路径，不包含空格或符号链接，祖先目录不可由其他用户写入；root 所有且带 sticky bit 的 `/tmp` 除外。

## 选择动作

| 动作 | 输入与行为 | 输出 |
| --- | --- | --- |
| `import` | `--file` 或安装记录中已有 OP；核验具名个人身份后导入 | 指定别名的本地 context；不签发证书、不授予权限 |
| `operator` | 手工初始化后的引导配置与 `--authority`；仅创建首个具名 OP | 超级管理员凭据及 context |
| `user` | 具名管理员配置、签发机构、姓名、角色和范围 | 独立个人凭据、角色绑定及 context |
| `node` | 具备平台权限的配置、全新 `--node` 和 `--output` | 节点交付目录；指定 SSH 时安装并验证心跳，节点组另行管理 |
| `browser` | 收件人的 `--file`、新 `--output` 目录 | `README.md`、`ca.crt`、`client.p12`、`token.txt` |

安装器通常已将 OP 导入操作用户默认配置；需要另一个 context 别名或配置文件时，用 `import` 复用该 OP。已有具名 OP 后新增管理员使用 `user --role super_admin`。`operator` 不用于重建或恢复已有管理员；凭据轮换和管理员恢复见[证书指南](../how-to/certificates.md)。

## 配置、范围与文件

| 参数 | 含义 |
| --- | --- |
| `--installation DIR` | 安装记录根目录；默认读取 `operator/config.json` 和 `bundle/user-authority`，不存在 OP 配置时才选择引导配置 |
| `--admin-config FILE`、`--admin-context NAME` | 显式指定管理身份及其 context；覆盖安装记录中的管理配置选择 |
| `--authority DIR` | 覆盖个人签发机构路径；只留在签发操作机 |
| `--contexts FILE` | 导入目标；默认 `CONVERYN_CONFIG`，再默认 `~/.converyn/config.json` |
| `--file FILE`、`--source-context NAME` | 导入或浏览器导出的源个人配置及其 context |
| `--context-name NAME`、`--use` | 目标别名及是否切换当前 context；别名不会改变服务端权限 |
| `--name NAME`、`--output DIR` | 新身份的显示名和凭据目录；不得复用已有输出 |
| `--role ROLE`、`--workflow ID`、`--preset NAME` | 业务权限范围；可重复指定预设；角色说明见[授权指南](../how-to/access.md) |
| `--all-workflows` | 明确授予业务角色全部流程范围，与 `--workflow` 互斥 |
| `--valid-for DURATION` | 个人证书有效期，默认 `365d`；仍受 CLI 和签发机构期限约束 |
| `--hardware-id-file FILE` | 目标 Agent 主机上已有的唯一身份文件路径 |
| `--ttl-seconds SECONDS` | 节点一次性引导有效期，60–3600 秒，默认 3600 秒 |
| `--password-file FILE` | P12 密码文件，0600；非交互浏览器导出必须提供 |

`viewer`、`executor`、`maintainer` 必须明确指定 `--workflow` 或 `--all-workflows`；只有前两者可限制 `--preset`。`platform_operator`、`access_admin`、`super_admin` 使用实例范围，不能附带流程/预设参数。`user` 完成签发后还会提交角色绑定；直接运行 `create identities` 则默认不授予业务权限。

个人输出中 `config.json` 可直接交给对应用户导入；不要发送管理机汇总了多人的 contexts 文件。节点交付目录包含 `converyn`、`install.sh`、`bootstrap.json`、`INSTALL.txt`、`checksums.sha256`。按 `INSTALL.txt` 在目标节点校验并安装，再检查认证心跳；业务节点组需另行更新。

## 中断恢复与常见问题

`operator`、`user`、`node` 在输出目录旁保存 `OUTPUT.init-plan.tsv` 及锁文件。中断后保留输出和记录，以相同脚本、二进制、身份及参数重跑并加 `--resume`；输入不一致会拒绝恢复。已有部分材料却缺少有效操作记录时需先核查，不删除后重新签发。

| 情况 | 处理 |
| --- | --- |
| `Expected ... 0700/0600` | 检查当前用户、目录祖先及文件权限；不要放宽凭据读取权限 |
| 同名 context 已存在 | 先检查该别名；确需替换时加 `--replace-context`，其他别名保留 |
| 导入失败或控制器不可达 | 检查所选二进制、端点、CA、具名证书及配套令牌；引导配置不能导入为个人身份 |
| `Business roles need ...` | 明确流程或全部流程范围，按角色选择预设限制 |
| 引导过期、已使用或 Node 已存在 | 核查原引导和节点状态；`--resume` 不延长引导期限，不自动恢复旧节点身份 |
| 浏览器输出目录已存在 | `browser` 不支持恢复覆盖；保留并核对原包，需要重新导出时选择新目录 |

自动化需明确动作并提供完整参数及 `--yes --non-interactive`；它们不会跳过权限或服务端授权检查。`--resume` 仅恢复原操作，证书轮换使用 CLI 的 `rotate credentials`。脚本选项完整列表以 `bash scripts/ops/init.sh --help` 为准，底层命令见 [CLI 参考](cli.md)。

## 维护者验证

```bash
bash -n scripts/ops/init.sh
python3 -m unittest discover -s scripts/tests/init -p 'test_*.py' -v
env CONVERYN_INSTALL_LIVE=1 python3 -m unittest discover \
  -s scripts/tests/init -p 'test_*.py' -v
```

真实 mTLS 夹具需要先构建 `target/debug/converyn`，并允许回环监听和子进程。它复用[安装器夹具](../development/testing.md)，验证首个 OP、独立个人证书、权限范围、context 保留、节点包和浏览器导出；不修改宿主 systemd，也不连接现有私有环境。

## 远程节点接入

指定 SSH 时先校验连接，再签发原始节点交付，安装后确认新鲜认证心跳。安装记录下默认使用 `nodes/UID`，也可指定新的 `--output`。密码或 sudo 密码在终端读取；无人值守使用已核验主机密钥及密钥认证。失败保留原记录，以原参数加 `--resume`，不更换已有 Node UID。

```bash
bash scripts/ops/init.sh node --binary /path/to/converyn \
  --installation /secure/converyn-install/qa --node node-b \
  --ssh-host 192.0.2.11 --ssh-user operator --ssh-auth key \
  --ssh-known-hosts /secure/known_hosts --yes --non-interactive
```
