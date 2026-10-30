# 原生流程目录与有序标签

流程目录由操作机读取，登记入口为 `workflow.yaml`。每个目录代表一个工作流；UID 决定身份，目录名不决定身份。目录路径可传给现有 `validate workflows --file` 和 `register workflows --file`。登记形成不可变快照，控制器之间复制定义和脚本，不扫描或同步本地目录。

## 目录与声明

```text
/opt/converyn/my-flow/
├── workflow.yaml
├── inventory.yaml
├── values.yaml
├── scripts/check.sh
└── lib/helper.sh
```

```yaml
uid: my-flow
name: Host validation
inventory: inventory.yaml
values: values.yaml
package:
  include: [lib]
targets:
  qa:
    hosts: all
playbook:
  api_version: converyn/v1
  name: host-validation
  steps:
    - id: check
      phase: rollout
      scope: per_node
      hosts: database
      max_parallel: 1
      operation:
        kind: exec
        script: scripts/check.sh
        timeout_seconds: 30
```

`workflow.yaml` 显式引用节点清单、普通变量和所需文件。`scripts/check.sh` 的实际内容在登记时计算摘要；不需要自己填写摘要或运行生成器。`values.yaml` 是普通 K/V 对象，沿用[变量规则](workflows.md#变量如何到达脚本)。大二进制、RPM 和 charts 使用独立制品，不内联到脚本包。

## 流程节点标签

`inventory.yaml` 例如：

```yaml
nodes:
  - uid: node-a
    address: 192.0.2.10
    hostname: db-a
  - uid: node-b
    address: 192.0.2.11
    hostname: db-b
groups:
  database: [node-b, node-a]
  monitoring: [node-a, node-b]
```

UID 必须指向已登记且未退役的 Agent；address 和 hostname 是业务拓扑，不用于替代节点认证。`groups` 是本流程独立标签，允许一个节点属于多个组和多个流程，与 controller、leader 等控制面角色无关。`all` 由 nodes 顺序派生，不重复声明。每个主机目标同时声明 `node_group_uid` 和 `hosts`。部署拓扑只来自 inventory；平台节点组提供允许范围，成员可以重叠。见[节点组](node-groups.md)。

首次批准目标、更换目标绑定的节点组或重命名目标，需要平台权限与流程维护权限。绑定不变时，维护者可在组内更新节点、地址、hostname、分组、顺序与业务内容。普通 `--set` 和运行配置文件不能替换清单和脚本身份。

选择器支持完整组、`database[0]`、`database[-1]` 和包含端点的 `database[0-1]`。索引越界和未知组报错。可选空组使用步骤 `allow_empty: true`；不声明时空选择失败。分组选择与 `roles`、`labels`、`node_uids` 不混用。

按列表顺序派发工作；`max_parallel: 1` 串行，其他并发值限制同时执行数量但不承诺完成顺序。目录默认采用整组 rollout；显式减小 `batch_size` 时，组顺序不能与目标节点的 rollout 批次顺序冲突，否则预览拒绝。现有活动 Run 固定节点列表，重新登记或 Leader 切换不会重排。

不同流程可以复用节点。活动 Run 对实际执行节点互斥；不相交节点可并行，资源所有者和控制面任务授权仍生效。节点组定义允许的节点范围，实际操作来自已批准的流程。

## 脚本与依赖

```bash
#!/usr/bin/env bash
set -euo pipefail
source "$CONVERYN_PACKAGE_ROOT/lib/helper.sh"
printf 'node=%s\n' "$CONVERYN_NODE_UID"
```

`package.include` 显式列出的 UTF-8 文件或目录形成只读依赖包，通过 `CONVERYN_PACKAGE_ROOT` 访问；任务工作目录独立可写。包按内容摘要缓存，不同修订不覆盖。源路径不得逃出流程目录，禁止符号链接和特殊文件。单个文件最多 64 KiB，最终流程修订和 Plan 各最多 1 MiB；超过限制的发行载荷使用下面的本地制品引用或显式外部制品源。

平台为脚本注入 `CONVERYN_NODE_UID`、`CONVERYN_RUN_ID`、`CONVERYN_WORK_ID`、`CONVERYN_OWNER_UID`；流程清单还提供 `CONVERYN_INVENTORY_JSON`，包含当前节点、业务地址、hostname、标签、有序节点及 groups。变量对象是 JSON 数据，不作为 Shell 源码求值。脚本发布结果使用标准 `task emit`，后续步骤通过声明的 inputs 读取，单节点生产者可写 `node: database[0]`。

## 声明式来源引用

可将制品配置放在 `artifacts.yaml`，manifest 使用 `artifacts: artifacts.yaml` 引入。`{$ref: /artifacts/archives/charts/artifact}` 引用完整来源文档的 JSON Pointer；`{$concat: [prefix, {$ref: /values/VERSION}]}` 仅拼接字符串。引用缺失或循环会报错，不执行表达式、Shell 或外部钩子。

```yaml
artifacts: artifacts.yaml
playbook:
  api_version: converyn/v1
  name: packages
  steps:
    - id: install
      for_each: /artifacts/packages
      phase: rollout
      scope: per_node
      hosts: all
      max_parallel: 1
      operation:
        kind: package
        manager: {$item: /manager}
        name: {$item: /name}
        version: {$item: /version}
        artifact: {$item: /artifact}
        timeout_seconds: 1800
```

`for_each` 只展开指定数组，生成 `install-0` 等稳定步骤 ID；`$item` 是当前元素的 JSON Pointer。空数组需显式 `allow_empty: true`。这是通用声明能力，适用于包、文件、检查等操作。

### 由 Converyn 分发本地制品

在原生 Binary 或 Package 操作的 `artifact` 中写入流程目录内的相对文件：

```yaml
operation:
  kind: binary
  path: /opt/example/tool
  mode: 493
  artifact: {$artifact: assets/tool}
```

同一引用也可放在 `artifacts.yaml` 中，再用 `$ref` / `$item` 选择。`validate workflows` 读取本地实际字节，计算 SHA-256 和长度；`register workflows` 在登记前自动上传所有本地制品。目录外路径、符号链接、空文件和超过 10 GiB 的文件均拒绝。文件不会作为 JSON、脚本文本或 Raft 日志内容内联；上传完成后删除操作机源目录不影响已登记版本。

冻结后的引用仅包含 `sha256`、`size`，省略 `locations` 表示由 Converyn 控制器分发。Agent 使用既有身份和本节点 Run 分配下载，继续实施缓存、断点续传、摘要和长度校验及本机写入权限。无需给 Agent 配置外部文件服务器。

登记时的摘要计算、制品总进度、已确认字节、文件数量、当前文件、有效上传速率和副本校验阶段显示在 stderr。终端内使用动态进度条，重定向时输出普通文本；stdout 保留原来的结果或 JSON。上传字节达到 100% 后仍可能等待副本校验，文件只在确认后计入完成数；重复引用已存在制品时标记为复用。


插件可直接声明 `kind: plugin`、`id`、`action`、`spec`，登记器使用本二进制内置契约解析，节点校验控制面批准的调用，无需本地授权片段。文本文件操作可用 `source: files/config.txt` 或 `content: ...`；也可使用 `files: {step-id: {json: /artifacts}}` 写入固定 JSON 内容。制品来源认证只配置在 Agent，不放到流程里。

## 登记、预览和边界

```bash
converyn validate workflows --file /opt/converyn/my-flow
converyn --context qa-op register workflows --file /opt/converyn/my-flow
converyn --context qa-op run workflows my-flow --target qa --dry-run
converyn --context qa-op run workflows my-flow --target qa \
  --request-id my-flow-001 --wait
```

`validate` 检查本地定义、文件和引用，requirements 列出操作及 Secret 需求；不验证远端 readiness。运行前控制器检查集中 Secret 与已知节点插件能力，Agent 自动执行批准的操作，无需合并本机权限片段。

Secret 在控制面加密保存，流程只登记名称和版本引用。不要把私有内容加入 package.include。已登记的文件快照不依赖操作机目录；更改内容后重新登记，已有 Run 保留原快照。

[可运行目录示例](../../../example/workflow-directory/README.zh-CN.md)

来源引用在登记时解析；固定制品身份、路径和 Secret 选择器所用的 values 应设为 immutable。操作需要运行时修改时使用 parameters.bindings。节点默认值按 inventory.nodes.values、workflow.node_values 的顺序合并，显式运行覆盖最后应用。全部步骤跳过的计划以 no_work_selected 拒绝。

Exec 可通过 fixed_environment 声明不可覆盖的步骤配置，例如共享入口的脚本选择。普通变量、节点默认值、--set、参数绑定和步骤输出绑定均不能替换这些名称。不要在 environment 与 fixed_environment 重复声明同名项；需要运行时调整的业务输入继续使用普通 values。

类似 #/definitions/type 的 URI/片段 $ref 保持为业务数据。需要原样传递含有来源指令形式的对象时，用 {$literal: ...} 包裹，内部不再解释。

execution.work_root 由监督器管理，业务和原生写权限不得覆盖该目录或其父目录。任务存储与业务可写目录应分开，避免其他任务修改缓存的依赖包。

隔离 Agent E2E 需要 root 验证任务切换到独立 UID，CI 通过 sudo 调用选定 Python 并保留工具 PATH。这些测试使用临时进程与替代安装动作，不访问现有部署节点。
