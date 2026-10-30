# 使用指南

[English](../../en/how-to/runs.md) | [简体中文](runs.md)

先完成[安装](installation.md)。本页是日常操作路径；全部选项见自动生成的 [CLI 参考](../reference/cli.md)，资源职责见[资源模型](../reference/resources.md)。示例中的 `RUN_ID`、`REVISION`、`WORK_ID` 需要替换为查询返回值。

## 选择身份和环境

```bash
converyn config import /secure/qa-alice/config.json --name qa-alice
converyn config get-contexts
converyn config use-context qa-alice
converyn config current-context
converyn --context qa-op status
converyn config view
converyn config export --output /secure/alice-portable.json
```

首次导入会选中，之后导入保留选择；`--use` 才切换。`--context` 仅对本次有效；也可 `--config FILE` 或设置 `CONVERYN_CONFIG` 指定另一份配置文件。`view` 隐藏私钥和令牌，`export` 输出含私有凭据的单 context 文件，拒绝覆盖。个人配置不得提交到仓库。

新人员默认无业务权限；授权决定可见流程、预设及任务日志。执行者控制自己的任务，维护者控制授权范围内全部任务；节点和控制面使用独立平台权限。详见[人员与流程授权](access.md)。没有人工审批阶段。

## 预览、参数与执行

```bash
converyn get workflows
converyn get workflows demo
converyn run workflows demo --target qa --set VERSION=two --dry-run
converyn run workflows demo --target qa --set VERSION=two --wait
```

`run workflows demo` 的流程 ID 是位置参数。终端、管道和 JSON 模式均默认提交后返回，显式 `--wait` 等待，默认超时 1800 秒；`--timeout-seconds` 仅与 `--wait` 配合使用，`--dry-run` 与 `--wait` 互斥。等待超时不取消服务器任务。Ctrl+C 只停止跟踪，原任务继续。多个预设必须指定 `--target`；一个预设可省略。预览校验目标、变量和计划，不执行操作，也不保证后续执行时目标状态未变化。

在仓库示例默认值之上覆盖本次变量：

```bash
converyn run workflows demo -f site.yaml -f temporary.json \
  --set image.tag=v2 --set-string VERSION=123 --wait
```

`site.yaml`、`temporary.json` 是自行准备的 YAML/JSON 对象文件。优先级：流程默认值 → 节点默认值 → 从左到右 `-f` → `--set` → `--set-string`。对象递归合并，数组、标量和 null 整体替换。`--set-string` 保留数字样式的版本字符串；`immutable` 字段不可覆盖。本次值不改变后续任务的默认配置。完整规则见[流程与脚本](workflows.md)。

## 请求重试与原任务

```bash
converyn run workflows demo --target qa --set VERSION=two --request-id qa-change-001 --wait
```

`qa-change-001` 只是示例，每次新的业务执行换一个 ID；同一次网络重试保持**同一 ID 和同一请求内容**。CLI 在提交前记录请求与 Run ID，并将原请求保存在客户端配置旁的 `requests/`，可用 `--request-dir` 或 `CONVERYN_REQUEST_DIR` 修改位置。不要把这类私有运行记录提交到仓库。

断连、503 或客户端退出不代表任务没有执行。先按已记录的 Run ID 查询，再决定是否重试原请求；不要通过新 ID 绕过未知结果。相同请求 ID 搭配不同内容会被拒绝。

## 查询任务、节点和日志

```bash
converyn status
converyn get runs
converyn get runs --status needs_attention
converyn get runs RUN_ID
converyn get runs RUN_ID -o wide
converyn watch runs RUN_ID --timeout-seconds 600
converyn --context qa-op get nodes
converyn --context qa-op get nodes node-a
converyn logs runs RUN_ID --step verify --node node-a --tail 200
converyn logs runs RUN_ID --step verify --node node-a --stream stderr
converyn logs runs RUN_ID --step verify --node node-a --follow
converyn logs runs RUN_ID --full --output-dir ./run-logs
```

Run 详情按步骤和节点展示操作、状态、退出码、原因及可复制日志命令；`-o wide` 显示完整 Work ID。节点在线只说明心跳新鲜，不证明任务成功。`get` 默认遍历全部分页；大历史可用 `--limit 50 -o json` 读单页，保留原过滤条件并用返回的 `next_cursor` 通过 `--cursor` 续查。

默认日志是每个流最后 100 行；`--follow` 用字节游标续读，Ctrl+C 仅停止查看。`--full` 校验已保存的完整日志，导出目录中的文件拒绝覆盖。HA 没有可用共享日志后端时，默认读取可能明确回退到回执尾部，完整读取会报错；每流 8 MiB 上限与保留期见[运维](maintenance.md)。原生操作可能只有结构化回执，没有文本日志。

浏览器流程页可直接进入“步骤与脚本”查看已登记内容；“执行配置”中的预览展示本次变量渲染后的脚本。任务页保留步骤列表，详情在宽弹窗中打开，关闭后回到原行。日志默认保持阅读位置，可开启“跟随最新输出”。出现存储故障时，页面会标明是否展示回执尾部；诊断信息保留错误码和请求编号，可重试故障恢复后继续按字节游标读取。

节点列表默认折叠其他 IP。资产和资源分别显示采集时效与完整性；“数据已截断”表示超过采集大小限制，不代表磁盘已满。审计默认展示业务操作与失败事件，可以切换查看节点状态上报或全部记录；筛选不删除原始审计。

`watch runs` 和 `run workflows --wait` 的退出码：

| 退出码 | 含义 | 后续动作 |
| --- | --- | --- |
| 0 | 成功 | 核对业务验收条件 |
| 2 | 失败、取消或被替代 | 查看原 Run 和回执 |
| 3 | 需要人工核实 | 由管理员检查原 Work 和现场 |
| 124 | 客户端等待超时 | 原任务继续，重新查询或等待 |

## 暂停、恢复、取消和核实

先 `get runs RUN_ID` 读取最新 `revision`。暂停/恢复使用修订号防止并发覆盖：

```bash
converyn pause runs RUN_ID --expected-revision REVISION
converyn get runs RUN_ID
converyn resume runs RUN_ID
converyn cancel runs RUN_ID
```

恢复时重新读取修订号。暂停阻止新授权，已获授权的工作继续；取消需要实际停止或完成证据，**不会回滚**已发生的变更。需要回退时，由 OP 编写明确的回退流程。

结果未知时，管理员核对原 intent、回执和目标主机后，使用详情中的完整 Work ID：

```bash
converyn --context qa-op resolve runs RUN_ID --work-id WORK_ID \
  --status succeeded --evidence '已核查原回执和目标实际状态，记录关联工单'
```

`--status` 只能根据证据选择 `succeeded`、`failed` 或 `canceled`，不能用它触发脚本重跑。

## 常用管理入口

| 任务 | 命令 |
| --- | --- |
| 登记/更新流程快照 | `validate workflows --file FILE`，再 `register workflows --file FILE` |
| 停止新的流程执行 | `disable workflows demo`；不取消原 Run |
| 查看人员及凭据 | `get identities`、`get credentials` |
| 查看控制面与节点组 | `get members`、`get node-groups` |
| 查看审计 | `get audit` |
| 导出逻辑备份 | `admin backup --output /secure/qa-backup.json` |
| 查看插件与外部目标 | `get plugins`、`get targets` |
| 创建浏览器登录资料 | `auth gui-login --output-dir /secure/gui-login` |

`update RESOURCE --file FILE` 提交带 action、修订和审计字段的类型化操作请求。管理操作使用具名 OP context。证书与登录资料见[证书指南](certificates.md)，备份恢复和升级见[运维](maintenance.md)。浏览器登录包默认只含说明和 CA，需要私有凭据时显式指定 `--with-credentials`，按受保护文件交付。

## 本地校验与插件查看

```bash
converyn get plugins
converyn get plugins --local
converyn validate workflows --file example/workflow
```

`get plugins` 读取所选控制器的能力，`--local` 查看当前二进制。本地校验、编译、准备和展开命令忽略环境中的客户端配置，并拒绝显式 `--config` 或 `--context`。操作机、控制器和 Agent 使用插件一致的构建。
