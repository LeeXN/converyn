# 贡献指南

[English](CONTRIBUTING.md) | [简体中文](CONTRIBUTING.zh-CN.md)

欢迎缺陷报告、文档修复及聚焦的改动。较大变更先说明问题和预期行为。运行于 Linux，使用 rust-toolchain.toml 固定的工具链及 C 编译器；编译 Rust 前先用 `npm ci --prefix web`、`npm run build --prefix web` 生成控制台资源；`web/dist` 是被忽略的构建产物。

项目处于初始开发阶段，只维护当前流程、配置和 CLI 契约，不增加仅用于兼容的分支或旧入口。契约变化同步修改调用方、示例和测试，并记录破坏性变化及升级步骤；更新时保护凭据、节点身份和执行证据。

```bash
npm ci --prefix web
npm run build --prefix web
cargo build -p converyn --locked --features full
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo test --workspace --all-features --locked -- --skip three_hundred_nodes_twenty_operators_and_large_history_baseline
```

测试前提、UT 与 E2E 命令见[测试说明](docs/zh-CN/development/testing.md)，固定 API/CLI 文档的检查也在其中。控制器状态、授权、fence、原回执和未知结果处理必须保持可审计，场景逻辑放插件，核心状态机不直接外部 I/O。

| 目录 | 责任 |
| --- | --- |
| crates/plugin-api | 插件契约与宿主能力 |
| crates/core | 领域、编译和确定状态转换 |
| crates/runtime | 控制器、Agent、持久化、凭据、执行 |
| crates/cli | 命令与二进制装配 |
| plugins | 编译期插件和脚本包 |
| web | 内嵌控制台 |
| example | 唯一示例入口，含完整 Kubernetes 流程 |
| scripts | 安装、初始化、操作机工具、构建与测试 |

## musl 静态发行构建

在 Linux x86_64 **构建机**上，从仓库根目录运行以下命令。需要 Python 3、Node.js 24/npm、`readelf`（binutils）以及提供 `musl-gcc` 的 C 工具链。例：Debian 可安装 `build-essential musl-tools binutils python3`；不同发行版的软件包名称可能不同。安装目标后仍需要 C 编译器，见 [Rustup 交叉编译说明](https://rust-lang.github.io/rustup/cross-compilation.html)。

```bash
# 按 rust-toolchain.toml 选择工具链，并安装 musl 标准库。
rustup show
rustup target add x86_64-unknown-linux-musl
command -v musl-gcc
command -v readelf

# 先生成 web/dist；HTML、JS、CSS 会在 Rust 编译时嵌入二进制。
npm ci --prefix web
npm run build --prefix web

# 输出目录必须不存在；脚本默认使用 full 插件集。
python3 tools/build_release.py --arch amd64 \
  --output release/linux-amd64 --build-tag my-amd64-build
```



```bash
sha256sum release/linux-amd64/converyn-linux-amd64 \

cat release/linux-amd64/manifest.json
readelf -l release/linux-amd64/converyn-linux-amd64 | grep INTERP || echo '没有动态解释器（静态链接）'
```

分别核对清单中的 `binary_sha256`、`installer_sha256`、`initializer_sha256`。依据清单及 SHA-256 识别构建，二进制与脚本应配套使用，已发布的版本不得以不同源码覆盖。构建脚本拒绝覆盖已有输出目录。


如只需要自行编译二进制，可在上述前端步骤之后运行：

```bash
cargo build -p converyn --release --locked --no-default-features \
  --features full --target x86_64-unknown-linux-musl
# 二进制：target/x86_64-unknown-linux-musl/release/converyn
```

该 Cargo 命令不生成发行清单和安装脚本；集中部署建议使用上面的 `build_release.py`。ARM64 构建需要对应 Rust target、Zig 和 readelf；构建机不能直接执行 ARM64 时还需要显式模拟器：

```bash
rustup target add aarch64-unknown-linux-musl
python3 tools/build_release.py --arch arm64 --zig /path/to/zig \
  --runner /path/to/qemu-aarch64-static --output release/linux-arm64
```

更新公开路由或 Serde 类型后重新生成 API、类型表、Web 和 Rust 产物；修改 CLI 后重新生成两种语言参考。生成和检查命令见[测试工具](docs/zh-CN/development/testing.md)。构建输出不包含仓库的 `example/`，首次流程示例需另行准备。安装入口见[安装器](docs/zh-CN/reference/install-script.md)和[初始化入口](docs/zh-CN/reference/init-script.md)。

## 文档

文档保持英文和简体中文成对更新，围绕功能、前提、输入、命令、输出和错误处理组织内容。使用简洁的操作说明与参考表格。API 和 CLI 参考通过对应生成器维护；示例使用文档地址和占位符，私有配置与运行记录存放于 `.local/`。

## Pull request

PR 说明问题、最终行为与验证；只加入相关源文件，不加入私钥、实际配置、运行数据、未经脱敏的截图或本地报告。提交贡献表示同意按项目 MIT 许可证提供。

文档主题分工见[文档索引](docs/zh-CN/README.md)：安装、常用操作和脚本契约分别维护，不复制第二套教程。旧实验脚本的精确忽略范围见[构建与测试](docs/zh-CN/development/testing.md)，当前回归测试继续保留。

## 版本发布

使用 `main` 持续开发，通过附注版本 Tag 发版。CI 自动构建并上传两种 Linux 架构的发行包，步骤见[发布指南](docs/zh-CN/development/releasing.md)。
