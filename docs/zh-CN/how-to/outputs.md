# 步骤输出与配置回写

[English](../../en/how-to/outputs.md) | [简体中文](outputs.md)

在生产步骤声明类型明确的 `outputs`，在后续消费步骤声明 `inputs`，用于同一 Run 内生成加入命令、令牌和发现业务配置。与 Ansible 的注册结果类似，输入来自生产者已提交的结果；Converyn 还冻结输入摘要，并将敏感值交付绑定到原 Node 授权。完整例子见[示例](../../../example/outputs/README.zh-CN.md)。

## 发布与消费

```yaml
outputs:
  join_command:
    type: string
    secret: true
    max_bytes: 4096
    ttl_seconds: 3600
```

```bash
printf '%s' "$join_command" |
  "$CONVERYN_EXECUTABLE" task emit --stdin --key join_command
```

```yaml
inputs:
  /environment/JOIN_COMMAND:
    step: create-token
    name: join_command
```

`task emit --stdin` 接收具名值组成的 JSON 对象；`--key NAME` 将原始标准输入包装为一个字符串。命令合并写入监督器创建的私有 `CONVERYN_OUTPUT_FILE`，不要 source 该文件。值是数据，不是可执行模板替换。每个声明字段都必须提供；未知字段、错误类型、不安全的所有者/权限、非法 JSON 或被脚本吞掉的写入错误，都会在脚本退出 0 后仍判定任务失败。只有经过核验的原始成功回执才能发布可消费输出，人工处置不能编造成功生产者结果。

引用可指定 `node`、`batch`、RFC 6901 `pointer` 及 `collect`。per-node 生产者默认选择同一节点，once-per-run 生产者须唯一确定；只能引用更早的执行波次。`collect: true` 返回有序的 `{node_uid,batch,value}` 对象。歧义、缺失、未来引用和冲突绑定被拒绝；选择其他 per-node 生产者时需明确源 Node。

普通值可绑定已有许可数据字段：脚本环境、文件 `/content`、服务/HTTP 选项及插件 spec 字段。不能修改资源键、文件目标、脚本身份、执行 profile、制品身份或目标身份；binary/package 制品保持固定。文件正文生成新的不可变定义和摘要，`.json` 目标要求有效 JSON。原生/插件普通输出通过 `source` JSON pointer 选择已核验回执证据。敏感输出由脚本产生；原生证据是公开证据，不是秘密通道。

## 敏感值、日志与密钥

Agent 使用 Run 的 P-256 公钥，通过 ECDH、HKDF-SHA256、ChaCha20-Poly1305 加密，认证绑定 Run、Work 和输出名称。共识状态、历史与状态备份只保存密文，不保存可公开验证的明文摘要。敏感引用只能绑定脚本环境字段。只有当前认证且持有已提交消费授权的指定 Node，才能通过现有 mTLS API 取得明文；个人管理员不能借该接口读取输出。响应带 `Cache-Control: no-store`，API 验收证据也不采集明文响应。

声明任意敏感输出后，整个生产步骤的 stdout/stderr 都不对外提供，包括实时上传、回执尾部、下载和导出；系统不解析 stdout 提取结果。消费步骤收到已知值的脱敏模式。脚本仍须管理自己明确调用的业务文件及外部命令，业务凭据文件应使用私有权限。

`admin init` 创建独立的 `output-keys.json`，复制到各控制器目录，服务配置通过 `output_keyring` 指向它。已有实例运行敏感输出流程前，需生成私有密钥环并向所有控制器安装同一套保留密钥：

```bash
converyn admin output-key --file /secure/output-keys.json
converyn admin output-key --file /secure/output-keys.json --rotate
```

命令只打印公钥描述。轮换一次后原子分发完整密钥环，保留旧密钥，不要在各控制器独立轮换。离线恢复副本与状态备份分开保管。缺失旧私钥会阻止旧密文消费，轮换本身不会使已有 Run 结果失效。

TTL 从生产者第一次提交成功回执时开始，在输入物化、新授权及私有交付时检查。物化/新授权时过期会使消费者失败并释放 Run 锁，交付时过期返回 410；缺密钥返回可重试的 503；密文、指针、类型或聚合大小非法返回永久 422。相同已接受授权的重试保留原授权，不创建替代执行，也不延长秘密有效期。

## 使用上下文的脚本

需要较多生成字段的脚本，可将声明的对象输入绑定到环境变量，再用本地命令调用不可变业务脚本：

```bash
"$CONVERYN_EXECUTABLE" task invoke \
  --environment-json-env INITIAL_CONTEXT \
  --environment-json-env PREVIOUS_CONTEXT \
  --fixed-json-env NODE_METADATA --publish-context --script ./business.sh
```

在 `business.sh` 内：

```bash
printf '{"discovered_address":"192.0.2.20"}' |
  "$CONVERYN_EXECUTABLE" task emit --stdin --context-update
"$CONVERYN_EXECUTABLE" task context --get discovered_address --raw
```

上下文更新命令在 `business.sh` 内运行。初始和后续上下文依次覆盖，聚合并发上下文遇到同名异值直接拒绝。固定元数据最后应用于子进程环境，不进入发布上下文。受保护的加载器/平台变量和超大环境被拒绝。命令不改变 UID、cgroup、文件系统授权或脚本身份，不提供网络服务、不持有个人 OP 凭据。使用 `--publish-context` 时须声明 `context` 对象输出；含凭据时应保护该输出。

上下文只存在于本 Run 声明的输出图中，不修改 Workflow 版本或全局节点组变量；新 Run 从声明的初始输入开始。持久业务配置由明确的原生文件/插件操作完成。流程分组与执行结果保留在本流程中；业务数据通过声明的 outputs 传递，操作不能写回全局节点角色。

## 恢复与限制

输入物化在 claim 前提交。原 Plan 保持不可变；`Attempt.materialized` 记录实际 Work、输入摘要及生产者承诺。Claim 必须回传输入摘要；Leader 切换、重试和重启复用冻结输入与原回执。精确完成回执持久化后才清理明文结果文件；未知结果私下隔离，不作为后续输入。

限制：128 个输出字段；每字段最多 16 KiB；声明文档 64 KiB；回执 256 KiB；每 Run 声明输出预算 4 MiB；敏感 TTL 1–86400 秒；解析后环境最多 128 KiB。字段默认 4096 字节、TTL 默认 3600 秒。有数据流的 Workflow 执行一次，不能同时声明持续核对。过期后应由用户明确发起新 Run，不自动重放非幂等操作。
