# 构建与测试

[English](../../en/development/testing.md) | [简体中文](testing.md)

本页说明源码构建、契约生成、公共回归和发行验收。用户安装与初始化见教程和脚本参考，业务脚本见流程编写指南。

## 目录归属

| 位置 | 内容 |
| --- | --- |
| `scripts/ops/` | 安装入口和操作工具 |
| `tools/` | 发布构建、生成器与契约检查 |
| `scripts/ops/kubernetes/` | 公共操作机输入工具 |
| `scripts/tests/e2e/` | 公共流程、原生输出、分页、包管理与浏览器验收 |
| `scripts/tests/fixtures/`、`scripts/tests/support/` | 制品/包夹具与共享配置读取模块 |

新增公共测试继续纳入版本管理。私有脚本和依赖只保留在维护机器，新检出仓库不包含它们，公共 CI 不得导入。运行凭据和环境清单仍留在 `.local/env/`。不要整体忽略 `test_*.py` 或 `*_e2e.py`。

## 准备与离线检查

从仓库根目录运行，使用 `rust-toolchain.toml` 固定工具链、C 编译器、Python 和 Node.js 24：

```bash
uv venv .venv
uv pip install --python .venv/bin/python -r tools/requirements-dev.txt
.venv/bin/python -m unittest discover -s scripts/tests/kubernetes -p 'test_*.py' -v
python3 tools/generate_api.py --check
python3 tools/render_api_docs.py --check
.venv/bin/python tools/check_api_contract.py
python3 tools/check_docs.py
npm ci --prefix web
npm test --prefix web
npm run build --prefix web
cargo build -p converyn --locked --features full
python3 tools/render_cli_docs.py --binary target/debug/converyn --check
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
```

前端需先构建，Rust 将 `web/dist` 内嵌进二进制。Python 开发依赖用于契约和辅助测试，不是产品运行依赖。

修改契约时去掉对应生成命令的 `--check`，更新 API、前端类型或 CLI 文档，再运行检查。生成来源与文档分工见[文档索引](../README.md)。

## Rust 与前端回归

```bash
cargo test --workspace --all-features --locked -- --skip three_hundred_nodes_twenty_operators_and_large_history_baseline
```

与 CI 相同，默认跳过容量基线；需要时在记录硬件的专用主机显式运行该用例。普通 runtime 测试包含临时目录、回环监听与进程隔离，需要环境允许这些能力；`ignored` 浏览器与真实包管理器用例只在显式准备的夹具中启用。不要在宿主包数据库运行 `package_live`。

## 安装与初始化回归

普通 Python discover 不会递归执行没有包入口的 `scripts/tests/init/` 和 `scripts/tests/install/`；与 CI 一样显式运行：

```bash
bash -n scripts/ops/install.sh
bash -n scripts/ops/init.sh
python3 -m unittest discover -s scripts/tests/install -p 'test_*.py' -v
python3 -m unittest discover -s scripts/tests/init -p 'test_*.py' -v
env CONVERYN_INSTALL_LIVE=1 python3 -m unittest discover \
  -s scripts/tests/install -p 'test_*.py' -v
env CONVERYN_INSTALL_LIVE=1 python3 -m unittest discover \
  -s scripts/tests/init -p 'test_*.py' -v
```

真实 mTLS 夹具使用已构建的 `target/debug/converyn`、临时目录和回环监听，替换 systemd/SSH 安装动作；不修改宿主服务或既有实验环境。覆盖范围见[安装器回归](testing.md)和[初始化说明](../reference/init-script.md)。

## 当前流程的真实验收

先构建 full 二进制，然后运行隔离临时目录与回环 mTLS 控制器/Agent：

```bash
python3 scripts/tests/e2e/workflow_e2e.py --binary target/debug/converyn \
  --output /tmp/converyn-workflow-result.json
python3 scripts/tests/e2e/workflow_e2e.py --binary target/debug/converyn \
  --controllers 3 --output /tmp/converyn-ha-workflow-result.json
```

检查实际文件/脚本、个人审计、请求去重、证书生命周期及旧写入口拒绝；HA 模式还在脚本实际写入后终止 Leader，验证原 Run 恢复且副作用仅发生一次。它不使用现有虚机。`--api-evidence /tmp/converyn-wire.jsonl` 以 0600 保存不含认证头的 HTTP 形状证据，再用 `.venv/bin/python tools/check_api_contract.py /tmp/converyn-wire.jsonl` 校验。

浏览器与分页检查需要明确选择 Chrome，报告文件使用新路径：

```bash
python3 scripts/tests/e2e/workflow_e2e.py --binary target/debug/converyn \
  --chrome /usr/bin/google-chrome --browser-report /tmp/converyn-browser-result.json
python3 scripts/tests/e2e/pagination_e2e.py --chrome /usr/bin/google-chrome \
  --output /tmp/converyn-pagination-result.json
```

浏览器通过隔离的客户端证书转发夹具访问真实 HTTPS 控制器，不能代替浏览器证书库导入的人工验收。CI 的显式 `browser` Rust 测试调用同一 `workflow_e2e.py`，不是另一套验收实现。Kubernetes 离线生成与回归见[回归说明](testing.md)；真实安装、重启、Docker/Kubernetes 使用专用可丢弃环境。

## 发行构建

`build_release.py` 生成静态二进制与来源清单；`plugin-build.rs` 是插件构建依赖。构建与分发命令见[贡献指南](../../../CONTRIBUTING.zh-CN.md)，部署后的运行证据按[运维](../how-to/maintenance.md)核对。

## 示例工具与回归位置

[操作机工具](../how-to/kubernetes-tools.md)负责原生 CLI 部署所需的输入准备；[Kubernetes 回归](testing.md)保存模拟和离线验收。`example/` 只保留用户可参考的流程、配置、业务脚本和演示应用。


## 真实 SSH 与 systemd 接入回归

此可选测试创建自己的容器、临时控制器和凭据；验证密钥 SSH、sudo、真实 Agent 安装、新鲜心跳及恢复后身份和证书保持不变。需要本机 Docker、可运行 systemd 的特权容器、可执行的临时目录及匹配的 Linux 静态二进制。不会接入既有平台。

```bash
docker build -t converyn-ssh-fixture:local -f scripts/tests/fixtures/ssh/Dockerfile .
env CONVERYN_SSH_LIVE=1 CONVERYN_SYSTEMD_IMAGE=converyn-ssh-fixture:local \
  CONVERYN_TEST_BINARY="$PWD/release/converyn-linux-amd64" \
  python3 -m unittest discover -s scripts/tests/init -p 'test_ssh_enrollment.py' -v
python3 tools/check_public.py --release release
```
