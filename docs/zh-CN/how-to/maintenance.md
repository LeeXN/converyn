# 运维与恢复

[English](../../en/how-to/maintenance.md) | [简体中文](maintenance.md)

首次部署见[安装](installation.md)，运行、日志命令和任务控制见[使用](runs.md)，账号与轮换见[证书](certificates.md)，本机执行策略见[流程编写](workflows.md)。本页集中说明诊断、存储、备份、升级和恢复。

## 运行诊断

| 证据 | 能证明什么 | 还需核对 |
| --- | --- | --- |
| systemd `active`、`/health/live` | 进程存活 | quorum、认证业务读取、实际任务结果 |
| `/health/ready` | 本成员的多数派就绪判断 | HA Leader 与各成员 metrics；Follower 本机可能 503 |
| `converyn status` | 经多数派确认的业务概况 | Leader 是单成员观察，不是逐台健康探测 |
| `get nodes NODE` | 缓存资产、资源与认证心跳 | 采集时间、过期/错误标记、实际回执 |
| Run 成功及原回执 | 已报告的执行结果 | 业务验收与目标实际状态 |

在各控制器使用 `journalctl -u converyn-controller`，Agent 使用 `journalctl -u converyn-agent`。本机 `admin agent-info --state-dir /var/lib/converyn/state` 可查看身份、活动意图和等待原因。接口使用 mTLS，不通过关闭证书验证排除认证问题。

失去多数派停止新授权；Agent 保留原 intent、grant、outbox 和回执，恢复后上报原结果。等待超时不停止任务，暂停不终止已授权工作，取消不回滚。脚本未知结果由管理员核查原 Work 后处置，命令见使用指南。

命令审计自动记录身份、凭据、流程和结果，与原始 stdout/stderr 分开。关闭集中日志不关闭审计；本地 `admin run-script` 是隔离实验入口，不产生控制器命令审计。

## 原始日志存储

生成的控制器配置默认没有集中日志后端。单控制器可使用本地目录，在 `server.json` 合并：

```json
{"log_store":{"backend":"filesystem","directory":"/var/lib/converyn-logs","shared":false,"retention_days":30}}
```

事先创建目录并授予控制器服务账号读写权限；安全修改配置并重启。HA 使用 S3 或所有成员可共同读写的真实共享目录，共享目录设置 `shared: true`，启动检查必须确认相同存储身份。不能把各机器上同名但独立的本地目录当共享后端。

S3 配置使用 `endpoint`、`bucket`、`region`、`prefix`、`access_key_file`、`secret_key_file` 和可选 `session_token_file`，字段见 [API 类型](../reference/api-types.md)。凭据以受保护文件给各控制器，不进入复制状态。

后端失败不阻止进程启动，上传独立退避；所以服务启动不证明完整日志可读。默认日志可明确回退到回执尾部，`--full` 和完整导出在后端不可用时报错。跟踪使用字节游标，`--follow -o json` 输出 NDJSON。每流最多保存 8 MiB，原始日志保留至少 30 天，过期后仍保留回执、摘要及缺失/到期标识。逻辑备份含日志清单，不含外部原始字节，后端需单独备份。

## 节点观测

资产在启动时采集，每五分钟检查；资源每二十秒采样。资产十分钟未检查、资源六十秒未采样则标为过期。缺失、不支持和采集错误明确记录，不填零。资产与资源上限分别为 64 KiB、16 KiB，超限记录截断。

序列号只是资产，不改变硬件绑定。采集不安装软件；NVIDIA 仅限时调用已存在的 `nvidia-smi`。节点在线/离线使用六十秒心跳窗口，不等同于设备健康或流程成功。

## 备份与同版本恢复

```bash
converyn --context qa-op admin backup --output /secure/qa-backup.json
sha256sum /secure/qa-backup.json
```

逻辑备份包含复制状态与冷历史，另外备份控制器配置、CA/签发材料、个人配置、Agent 身份和执行目录以及原始日志后端。私有备份使用受保护目录与权限。

恢复必须先实际隔离旧控制面，防止两套实例向同一目标执行；使用与备份匹配的二进制和全新目录。下面的 `--old-control-plane-fenced` 是操作者对已完成隔离的声明，**不是自动隔离命令**：

```bash
converyn admin restore --backup /secure/qa-backup.json \
  --data-dir /var/lib/converyn-restored --node-id 1 --old-control-plane-fenced
```

将恢复目录配置为对应成员的 `data_dir`，为替代 quorum 准备正确成员配置和凭据，再启动并初始化。核对成员、具名身份、暂停的原任务、原回执及资源所有权，完成核对后才解除维护、恢复授权。完整选项见 [CLI](../reference/cli.md)。不要复制单个运行中的 SQLite 文件冒充逻辑备份。

## 升级与新实例替换

升级前等待无在途执行并备份，保存每台主机的旧二进制、配置、状态和服务文件。比较发布清单的协议、schema、`plugin_manifest_sha256` 和二进制 SHA-256，不能只看版本字符串。

| 情况 | 处理 |
| --- | --- |
| 协议/schema/插件清单兼容的增量更新 | 按发布约束先全部控制器、验证复制和业务读取，再更新 Agent |
| 插件清单不同或协议不兼容 | 协调停监督器和控制器，统一切换，不混跑不同清单 |
| 不支持的旧业务 schema | 新实例新目录，保留旧版本及其原始数据，不直接接入旧库 |

更新后核对 `/proc/PID/exe` 实际摘要、各成员一致性、quorum 业务读取、新鲜 Agent 心跳，以及原任务和新任务的实际回执。写入新格式后不可直接降级到不能解析它的旧二进制；向前修复，或隔离后恢复配套旧备份。

同实例升级/恢复应保留 Agent 身份及执行证据。独立替换实例则使用新的配置、凭据和登记目录，先停止旧控制面对同一目标发工作，不复制旧 Agent 执行目录到新身份，不自动接管资源。原凭据、数据库、日志和备份均保留。

### 二进制统一升级：标准 systemd 安装

本流程更新同一个实例，不重新初始化、登记或替换节点。先比较旧、新发行清单中的 schema；不同 schema 须先确认对应迁移路径，不能直接把新程序接入旧库。同版本字符串不能证明构建相同。发布准备见[静态构建](../../../CONTRIBUTING.zh-CN.md#musl-静态发行构建)。

#### 1. 盘点与冻结工作

在每台主机记录实际安装的角色、启动程序、配置路径和服务状态：

```bash
systemctl show converyn-controller -p LoadState -p ActiveState -p ExecStart -p WorkingDirectory
systemctl show converyn-agent -p LoadState -p ActiveState -p ExecStart -p WorkingDirectory
```

下面按标准路径说明：二进制为 `/usr/local/bin/converyn`，控制器配置和数据在 `/var/lib/converyn-controller`，Agent 配置在 `/etc/converyn/agent.json`，状态默认 `/var/lib/converyn/state`。自定义部署按实际 ExecStart、data_dir 和 state_dir 替换路径，不移动状态目录。

在操作机使用当前运行构建匹配的 CLI 和已有个人 OP 配置：

```bash
ADMIN_CONFIG=/secure/converyn-install/qa/operator/config.json
converyn --config "$ADMIN_CONFIG" get runs
converyn --config "$ADMIN_CONFIG" get workflows
converyn --config "$ADMIN_CONFIG" admin backup --output /secure/before-update.backup
sha256sum /secure/before-update.backup
```

所有非终态 Run（包括准备中、暂停、取消未确认和 needs_attention）都须先完成或核查，不能为升级盲目重放或强行清理。维护期间禁止新提交；记录已有启用流程，必要时用 `disable workflows UID` 暂时停用，验收后只恢复原来启用的流程。停用不取消已经授权的工作。

逻辑备份客户端会校验内部载荷；另外保存整个文件的 SHA-256，并保留操作机的原安装记录、CA/签发材料和个人 context。备份输出必须是新文件，/secure 目录须先准备为私有可写目录。

#### 2. 先分发与校验，再停服务

将同一架构、同一构建的 full 发布二进制复制为每台主机的 `$HOME/converyn.new`。确认发布清单中的 binary_sha256，替换下面占位值后执行：

```bash
EXPECTED_SHA256='REPLACE_WITH_MANIFEST_BINARY_SHA256'
printf '%s  %s\n' "$EXPECTED_SHA256" "$HOME/converyn.new" | sha256sum --check
chmod 755 "$HOME/converyn.new"
"$HOME/converyn.new" -o json get plugins --local
```



#### 3. 协调停机并冷备份

协议或插件清单不同，先停止所有已安装的 Agent，再停止全部控制器；每条命令仅在装有相应角色的主机执行：

```bash
sudo systemctl stop converyn-agent
```

```bash
sudo systemctl stop converyn-controller
```

确认服务退出、MainPID 为 0，才在每台主机备份。下面命令适用于 Linux/GNU tar，会跳过本机未安装角色的目录，备份目录不会覆盖旧记录：

```bash
sudo bash <<'SH'
set -euo pipefail
umask 077
backup_dir=$(mktemp -d /root/converyn-upgrade.XXXXXXXX)
paths=(usr/local/bin/converyn)
for path in \
  etc/converyn \
  var/lib/converyn \
  var/lib/converyn-controller \
  etc/systemd/system/converyn-agent.service \
  etc/systemd/system/converyn-controller.service
do
  [[ ! -e /$path ]] || paths+=("$path")
done
tar -C / --xattrs --acls --selinux -czf "$backup_dir/state.tgz" "${paths[@]}"
tar -tzf "$backup_dir/state.tgz" >/dev/null
sha256sum "$backup_dir/state.tgz"
printf 'Private backup: %s\n' "$backup_dir"
SH
```

若配置了自定义数据目录、Secret 目录、硬件身份文件或原始日志后端，还须备份实际位置。运行中的 SQLite 文件复制不等于有效冷备份。停机窗口内保持旧、新服务不混跑。

#### 4. 原子替换与配置核对

各主机在同一维护窗口内执行；命令再次校验安装后的临时文件，然后在同一文件系统内原子替换，并在 SELinux 主机恢复安全标签：

```bash
sudo bash -s -- "$HOME/converyn.new" "$EXPECTED_SHA256" <<'SH'
set -euo pipefail
source_binary=$1
expected_sha256=$2
[[ $expected_sha256 =~ ^[0-9a-f]{64}$ ]]
[[ -f $source_binary && ! -L /usr/local/bin/converyn ]]
install -o root -g root -m 0755 "$source_binary" /usr/local/bin/converyn.next
printf '%s  %s\n' "$expected_sha256" /usr/local/bin/converyn.next | sha256sum --check
mv -f /usr/local/bin/converyn.next /usr/local/bin/converyn
if command -v restorecon >/dev/null; then restorecon /usr/local/bin/converyn; fi
SH
```

不要重新运行安装向导或以 `install.sh --resume` 做版本升级，不发新 bootstrap，不修改 node_uid、cluster、TLS、令牌、硬件绑定、state_dir 或既有回执。单纯换二进制不需要 daemon-reload。

比较新 `get plugins --local` 描述；插件流程从原生源码重新登记，不能直接把带旧 call 身份的导出定义当新源码使用。节点无需插件授权文件。若配置了控制器外部连接，核对对应插件 identity、允许目标及连接凭据，保留其他已有配置。

#### 5. 控制器先恢复，再恢复 Agent

先在全部控制器上启动服务；不要在只启动第一个控制器时等待整组就绪：

```bash
sudo systemctl start converyn-controller
```

```bash
converyn --config "$ADMIN_CONFIG" status
converyn --config "$ADMIN_CONFIG" get members
```

```bash
sudo systemctl start converyn-agent
```

中间两条诊断命令在操作机执行。确认 Leader、多数派业务读取、选民与成员地址符合原记录、Leader 的复制进度覆盖全部成员，再启动 Agent。逐台控制器指标接口为 `/api/v1/controlplane/metrics`，配合 `/health/live`、`/health/ready` 及认证业务读取检查；Follower 本机 ready 返回 503 不等于整个集群故障。

#### 6. 核对实际运行与回执

按本机角色核对实际进程摘要，不只看磁盘文件；MainPID 必须非 0：

```bash
pid=$(systemctl show converyn-controller -p MainPID --value)
sudo sha256sum "/proc/$pid/exe"

pid=$(systemctl show converyn-agent -p MainPID --value)
sudo sha256sum "/proc/$pid/exe"

converyn --config "$ADMIN_CONFIG" get nodes
sudo journalctl -u converyn-controller -n 100 --no-pager
sudo journalctl -u converyn-agent -n 100 --no-pager
```

各运行进程的摘要须与发布清单相同。Agent 应出现新的会话与新鲜心跳，Node UID、硬件摘要、证书和令牌保持不变，已有身份、任务和回执仍可读取。使用一套只读验收流程在目标节点实际执行，核对每个成功回执和非 root 执行身份，再停用验收流程、保留审计和恢复原流程启用状态。

后续新增节点显式使用新发布二进制，例如 `--binary /usr/local/bin/converyn`，并配套新发布目录中的两个 Shell 脚本；原安装记录继续作为凭据和恢复材料保存。

#### 故障与回退

失败时保留维护窗口与备份，按 journal 和私有诊断确定故障，不重建身份或清空状态。已经写入新模型后不能只换回旧程序；应先统一停止并隔离新控制面，再按匹配的旧二进制、配置和完整备份恢复。只恢复一台旧控制器或让不同协议/清单混跑不是回退方案。

## 有审计的历史清理

仅在明确需要时，由管理员先导出并验证备份，再从 `get history` 读取 `run_sequence`。通过 `update history --file clear.json` 提交：

```json
{
  "action": "clear_runs",
  "expected_sequence": 123,
  "backup_sha256": "<已验证备份的64位SHA256>",
  "evidence": "明确的清理请求与关联记录"
}
```

序号必须一致，且没有执行中或待核实工作。清理热/冷 Run 及执行快照，保留身份、流程、所有权、fence、请求墓碑、安全/命令审计和外部备份。已清理请求 ID 不可重放；日志索引保留至旧输出回收完成。

敏感输出的加密密钥环独立于状态备份；保留旧密钥，并向所有控制器分发同一完整密钥环。轮换及恢复要求见[输出契约](outputs.md)。


## 恢复执行前的输入失败

Run 因输入物化失败而停止、失败与取消的工作都没有取得执行许可时，可以使用 `converyn resume runs RUN_ID`，或网页中的“恢复未执行的输入步骤”。系统保留成功回执和生成的加密输出，只重排从未执行的工作，并重新检查节点组范围、节点占用与资源归属。已执行后失败、状态不明或同一资源归属已有更新的执行，不允许通过此操作重放。
