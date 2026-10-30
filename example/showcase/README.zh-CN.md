# 平台功能综合展示

[English](README.md) | [简体中文](README.zh-CN.md)


一条流程展示目录插件、真实 deb/RPM、控制器制品分发、文件交付、systemd 服务、HTTP 检查、脚本、类型化输出和版本化 Secret。使用已登记的 Linux/systemd 测试节点、full 二进制、Python 3 和 curl。示例只管理自己的演示包、服务和 `/opt/converyn-showcase`。平台管理员预先准备节点组及流程权限。

## 准备与运行

私下准备 `/secure/showcase-token`（0600），不要提交内容。演示包没有安装脚本，只写一个数据文件；准备工具不会安装软件包。先完成[快速开始](../../docs/zh-CN/tutorials/quickstart.md)。

```bash
uv venv .venv
uv pip install --python .venv/bin/python PyYAML==6.0.2
.venv/bin/python scripts/ops/showcase/prepare.py --manager deb --output .local/showcase
# Use --manager rpm when the target hosts use RPM; rpmbuild must be installed on the build host.
# Edit .local/showcase/inventory.yaml with enrolled Node UIDs, addresses and hostnames.
converyn --context qa-op register secrets showcase-token --version v1 --file /secure/showcase-token
converyn validate workflows --file .local/showcase
converyn --context qa-op register workflows --file .local/showcase
converyn --context qa-op run workflows feature-showcase --target default --dry-run
converyn --context qa-op run workflows feature-showcase --target default --request-id showcase-001 --wait
converyn --context qa-op get runs run-showcase-001
converyn --context qa-op logs runs run-showcase-001
```

## 验证与清理

成功回执应包含软件包版本、制品摘要、HTTP 内容和输出验证。验证脚本只检查 Secret 是否存在，不输出其内容。服务以 nobody 身份监听回环端口 18081；不需要旧式 Agent 授权片段。

停止使用流程后，通过主机包管理器及 systemd 移除 `converyn-showcase.service`、演示软件包和 `/opt/converyn-showcase`，保留 Converyn 状态。重启和持续核对分别见[维护](../maintenance/README.zh-CN.md)与[持续核对](../reconciliation/README.zh-CN.md)。
