# 人员、用户组与流程授权

[English](../../en/how-to/access.md) | [简体中文](access.md)

Converyn 使用平台权限与流程授权，不设置项目空间。不同环境可独立部署；同一实例也可以通过不同流程和目标预设管理多个环境。Context 选择客户端连接及个人身份，不承担服务端权限隔离。

## 默认规则

新人员没有任何业务授权。角色绑定把人员或用户组、角色、流程及预设范围关联起来。多份绑定累加，没有显式拒绝规则。权限更改影响后续读取和提交；撤销不会自动取消已提交任务。任务、回执、日志、完整下载和归档记录使用任务原始流程及预设的授权。

| 角色 | 能力 | 范围 |
| --- | --- | --- |
| viewer | 查看流程、任务、回执和日志 | 指定流程及可选预设，或全部流程 |
| executor | viewer，加预览、执行及控制本人任务 | 同上 |
| maintainer | executor，加登记、启停及控制范围内所有任务 | 指定流程或全部流程，必须覆盖该流程全部预设 |
| platform_operator | 管理节点、节点组、外部目标及控制面，读取平台审计 | 实例 |
| access_admin | 管理普通人员、用户组、凭据及角色绑定 | 实例 |
| super_admin | 完整管理权限 | 实例 |

平台运维与授权管理权限不自动包含业务执行能力。只有超级管理员可以授予、移除超级管理员，或者修改其人员、组成员及凭据。系统保护最后一位启用的超级管理员及其最后一份有效凭据。第一版使用固定角色，不提供自定义角色或委派授权。

维护流程意味着可以修改它的脚本、目标绑定及固定参数，应作为高权限授予。人员授权仍受 Agent 路径、执行账号、插件连接和目标端权限限制。

## 创建人员并授权

先按[证书指南](certificates.md)创建个人凭据，再查询稳定人员编号：

```bash
converyn get identities
converyn auth group release-team --name '发布团队' --member USER_ID
converyn auth grant release-test --group release-team --role executor --workflow deploy --preset test
converyn auth grant workflow-owner --person USER_ID --role maintainer --workflow deploy
converyn auth list
```

将 `USER_ID`、流程与预设替换为实际值。人员和组选择支持稳定 UID 或唯一名称；服务端保存稳定 UID，重名时必须使用 UID。`auth group` 设置完整成员列表；再次调用会替换成员，不是追加。省略 `--workflow` 授予全部流程；省略 `--preset` 授予该流程全部预设，包括后续新增预设。指定预设时必须同时指定一个流程，maintainer 和平台角色不接受预设范围。

GUI 的“访问管理”提供人员添加与启停、组成员编辑、授权范围预览及撤销。GUI 新增人员没有登录凭据，需再通过 `create credentials --identity USER_ID` 签发；私钥和令牌不进入控制器复制状态。

## 自查、撤销与并发更新

```bash
converyn auth doctor
converyn auth status
converyn auth can-i execute --workflow deploy --preset test
converyn auth revoke release-test
```

`can-i` 允许时退出 0，拒绝时退出 1，不执行业务动作。`auth status` 展示直接及用户组来源的有效绑定。GUI 隐藏无权进入的平台页，业务列表只返回授权范围；仅有查看权限的预设不能提交预览或执行。

所有授权变更带 `expected_revision`。便捷命令自动读取修订号；冲突时刷新并重新审查后提交。需要声明式变更时使用 `converyn auth apply -f access-change.yaml`：

```yaml
action: put_binding
expected_revision: 3
binding:
  uid: release-test
  subject:
    kind: group
    uid: release-team
  role: executor
  workflow: deploy
  presets: [test]
```

分页游标绑定查询条件、人员及授权状态。授权或成员发生变化后，旧游标失效，应从首页重新查询。日志跟踪和下载在后续读取时重新检查授权。
