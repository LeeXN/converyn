# 节点组与流程范围

[English](../../en/how-to/node-groups.md) | [简体中文](node-groups.md)

平台节点组定义允许范围；流程 inventory 定义部署拓扑、业务分组和顺序。节点 UID 连接登记身份、平台范围与流程。一个节点可以加入多个节点组，这不承诺生产与测试物理隔离。

从[快速开始](../tutorials/quickstart.md)完成安装、导入个人 context、接入 node-a 后，由平台运维创建节点组：

```bash
converyn --context qa-op update node-groups --file example/node-group.yaml
converyn --context qa-op get node-groups example-hosts
```

创建动作是 `create_node_group`，`expected_revision: 0`，组修订为 1。更新动作是 `update_node_group`，携带当前 `expected_revision`，新修订加一。UID 不可修改，`name` 只用于展示，`node_uids` 是无序集合。允许空组；实际目标选择必须非空。所有成员必须已登记且未退役。

`example/workflow/workflow.yaml` 的目标同时声明 `node_group_uid: example-hosts` 和 `hosts: all`。`qa` 是流程作者取的目标预设名称，不是保留字。选择器只解析该流程 inventory；平台组名不会变成流程分组。平台增加成员不会自动扩大部署。

首次登记主机流程、新增主机目标、改变目标绑定组、重命名目标，需要同一调用者同时具备该流程的 `maintainer` 与平台 `platform_operator` 权限；`super_admin` 也具备这些能力。平台权限本身不授予业务执行权限。已批准的“目标名＋组 UID”绑定不变时，维护者可自行更新组内节点、地址、hostname、分组、顺序和步骤。删掉目标后重新添加仍按新绑定审批。删除最后一个目标后，流程保持未绑定状态，维护者仍可查看；重新批准目标前无法执行。

```bash
converyn --context qa-op register workflows --file example/workflow
converyn --context qa-maintainer register workflows --file example/workflow
converyn --context qa-alice run workflows demo --target qa --dry-run
converyn --context qa-alice run workflows demo --target qa --request-id demo-scope-001 --wait
converyn --context qa-alice get runs run-demo-scope-001
converyn --context qa-alice logs runs run-demo-scope-001
```

`qa-maintainer` 需先由访问管理员授予 demo 的维护权限。运行者继续按“流程＋目标预设”授权。`--set`、参数绑定及运行配置不能改节点组绑定。本地 `validate workflows` 和离线编译只检查结构与选择器，平台成员关系尚未核验。服务端登记校验所有目标；预览、提交和准备后准入重查当前目标。越界报错，包含组、目标与越界节点，不会静默过滤。

预览到提交之间组修订变化须刷新预览。Run 固定流程版本、节点与顺序；提交后的无关成员变化可继续。准备期间所需成员被移除会以 `scope_invalid` 失败，须重新预览提交。已准入且仍占用节点的任务禁止移除所需成员，包括暂停、取消处理中和 `needs_attention`。实际节点与外部目标分别加锁，节点组不加执行锁：同组不同节点可并行，跨组使用相同节点仍冲突，资源归属检查也不因换组而绕过。

自动协调遇到 `scope_invalid` 停止自动重试。修复范围后，由维护者重新预览、提交或恢复协调配置。维护、维护窗口、资源归属和身份恢复依据节点登记与执行占用，不依赖成员关系。移出节点组不清除身份、维护状态、资源归属或历史。节点退役会检查全部节点组及执行占用。灾难恢复隔离所有有效登记节点，包括没有组的节点。

```bash
converyn --context qa-op delete node-groups obsolete-hosts --expected-revision 3 --reason 'environment retired'
```

当前流程定义仍引用的组不能删除，包括禁用流程。删除保留退役标识及审计，UID 不可复用。普通流程用户只看到授权目标的 inventory，不获得整个平台组清单。列表分页上下文绑定权限和范围变化。HTTP 使用 `GET /api/v1/node-groups` 和 `POST /api/v1/node-groups/commands`。

当前存储 schema 为 23，通信协议为 27，Plan 格式为 4。旧存储、组件、部署字段和 `clusters` 命令/API 明确拒绝；不提供历史转换。控制面实例身份、Raft 成员及 Kubernetes 业务集群身份保持各自含义。
