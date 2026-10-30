# 个人证书与令牌

[English](../../en/how-to/certificates.md) | [简体中文](certificates.md)

OP 通过 CLI 签发和轮换成套凭据；GUI 可管理人员启停和授权。网络管理命令使用具名管理员证书及令牌，签发还需要本地 user-authority。普通用户不持有签发材料。若已按安装指南创建 Alice，可直接从浏览器导入开始；下面创建命令只用于新身份。


首次部署推荐使用[初始化脚本](../reference/init-script.md)：`import` 复用安装器 OP，`user` 完成个人签发、角色绑定和 context 导入，`browser` 导出对应用户的 P12/令牌/CA 及登录说明。完整命令和私有目录准备见[快速流程](../tutorials/quickstart.md)。下面保留底层 CLI 的凭据生命周期操作；不要为同一人重复创建身份。
## 创建和列出

```bash
converyn config import /secure/qa-op/config.json --name qa-op --use
export CONVERYN_AUTHORITY=/secure/converyn-qa/user-authority
converyn create identities --name Alice --valid-for 365d --output /secure/qa-alice
converyn get identities
converyn get credentials
```

创建命令返回稳定人员 ID；后续 --id 或 --identity 使用该 ID。新人员默认无业务权限，--super-admin 创建超级管理员；普通人员需另外授予[流程权限](access.md)。输出目录必须不存在，权限为 0700；config.json 是可直接导入的自包含个人配置；私钥、证书、token、config.json、credential.json 和 operation.json 均以受保护文件保存。列表不返回令牌或私钥。

个人证书默认 365 天，最小五分钟，最长 1095 天。--valid-for 支持 s/m/h/d，或使用 --expires-at 指定带时区的 RFC3339 时间，二者互斥。证书有效期不能超出签发机构剩余有效期；超出时直接报错。Node 和控制器的证书期限仍由 init/pki-issue 的 --leaf-lifetime-seconds 管理，不套用人员三年上限。

## 浏览器导入

```bash
converyn export credentials --config /secure/qa-alice/config.json --current \
  --output /secure/qa-alice/browser.p12
```

该可选导出操作需要 OP 主机安装 OpenSSL，并交互输入导出密码；也可用 --password-file 指向受保护文件。密码不放在命令行参数中。

把 browser.p12 和公开 CA 复制到运行浏览器的电脑。Windows 通过 certmgr.msc 导入当前用户“个人”证书库，再核对并信任 CA。浏览器访问控制器 HTTPS 地址，选择个人证书，填写控制面名称和对应 token 文件内容。证书包导出密码只用于导入，不是登录令牌。其他系统使用浏览器或操作系统个人证书库。

## 修改、续期、轮换、删除

```bash
converyn update identities USER_ID --name 新名称
converyn rotate credentials --identity USER_ID --valid-for 730d \
  --output /secure/qa-alice-new
converyn rotate credentials --identity USER_ID \
  --expires-at RFC3339_WITH_TIMEZONE --overlap 24h \
  --output /secure/qa-alice-next
converyn revoke credentials CREDENTIAL_ID
converyn disable identities USER_ID
converyn enable identities USER_ID
```

上面的绝对到期时间只是格式示意，应替换为距当前不超过 1095 天且未超出 CA 有效期的时间。

已经签发的证书不能原地修改；有效期和密钥变更通过 rotate 重新签发。证书、私钥、令牌一起更换，人员 ID 不变。旧证书配新令牌或新证书配旧令牌都被拒绝。

默认提交轮换后旧凭据立即失效；显式 --overlap 允许最长七天重叠。截止后服务端直接拒绝旧凭据，不依赖清理任务。再次轮换不能延长旧截止时间。用户需要同时替换浏览器证书和个人令牌。

delete 撤销一套凭据，不删除历史证据；disable 停用整个人员的全部凭据。仅剩一个启用管理员时不允许停用该身份。保管好另一套管理员与离线恢复材料，撤销访问不会取消已接受的任务。

## 中断恢复与秘密边界

新凭据包先落盘，再提交注册或轮换。结果未知时：

```bash
converyn resume credentials --output /secure/qa-alice-new
```

恢复读取原 operation.json 并检查新凭据是否已生效，不重新生成密钥或重复延长重叠期。不要覆盖或删除原输出目录。

控制器只存公开证书、指纹及令牌摘要。API 在登记前检查证书链是否受控制器信任。客户端证书验证和 API 令牌绑定同时生效；长期流式日志也持续复核有效身份。审计不保存令牌明文、私钥或导出密码。

## 管理员凭据恢复

仅当全部具名管理员凭据均不可用、且引导证书和令牌仍有效时，OP 可使用本地引导配置为已有管理员重新签发；不能借此创建普通用户或修改流程。

```bash
converyn rotate credentials --config /secure/converyn-qa/admin/config.json \
  --authority /secure/converyn-qa/user-authority --identity OP_ID \
  --output /secure/op-recovered
```

恢复仍保留原人员 ID 和审计。正常管理始终使用具名凭据；引导材料保留在 OP 受保护目录。

## 查询详情、位置与导出

```bash
converyn get identities OP
converyn get identities USER_ID
converyn get credentials --identity QA
converyn get credentials CREDENTIAL_ID
```

身份详情列出关联凭据；证书详情展示有效期、撤销状态、指纹、序列号、服务端保存位置，以及当前本机配置是否持有匹配凭据。人员名称必须唯一，否则使用 UID。控制器登记公开证书和令牌摘要，不保存个人私钥、令牌明文或签发工作站原始目录；无法从控制器下载他人的私钥。公开证书可按登记 ID 导出：

```bash
converyn export credentials --id CREDENTIAL_ID --output ./public.crt
```

已导入 context 的个人凭据可以直接导出浏览器 PKCS#12，无需找到原始签发目录：

```bash
converyn export credentials --current --output ./browser.p12
converyn --context qa export credentials --current --output ./qa.p12
converyn config export --output ./personal-context.json
```

PKCS#12 包含选中本机凭据的证书与私钥，需要导出密码；不包含 API 令牌。`config export` 的受保护 JSON 包含配套令牌、证书与私钥，可供 CLI 导入。它们都属于私有文件，不要公开。证书来源 `--id`、`--current`、`--ca` 互斥；所有导出都拒绝覆盖已有文件及符号链接。详情中的本机路径只说明当前选中配置，不能证明其他工作站上的副本位置。

## 直接导出 CA 与生成 GUI 登录交付说明

```bash
converyn export credentials --ca --output ./ca.crt
converyn --context single export credentials --ca --output ./single-ca.crt
converyn auth gui-login --output-dir ./gui-login
```

`--ca` 导出所选客户端配置的公开 CA PEM 证书包，保留其中全部 CA，返回各证书 SHA256 和有效期，不包含 CA 私钥。已有文件和符号链接均拒绝覆盖。

`gui-login` 通过 mTLS 确认当前具名人员后生成新目录，默认只有 `README.md` 与 `ca.crt`。说明包含实际访问地址、控制面名称、人员、权限、证书指纹/期限、Windows 证书导入、浏览器登录步骤和常见故障处理。不会生成新身份或轮换凭据。需要英文说明时加 `--language en`；默认中文。

向某个用户交付完整登录材料时，显式选择该用户自己的配置，再启用凭据打包：

```bash
converyn --config /secure/qa-alice/config.json auth gui-login \
  --output-dir ./alice-gui --with-credentials
```

此时目录还包含 `client.p12` 和 `token.txt`，属于该人员的私有交付包，目录权限为 0700、文件为 0600。OpenSSL 交互询问 P12 密码；自动化可加 `--password-file /secure/export-password.txt`。令牌、私钥和密码不会写入 README，也不会打印在命令输出中；P12 密码不保存在包中，需另行提供。不要用当前 OP 管理员配置替普通执行者打包。命令拒绝已有目录（包括空目录及符号链接），任何生成失败均不发布半成品目录。输出目录的父目录须已存在。
