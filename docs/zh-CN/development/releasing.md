# 发布版本

[English](../../en/development/releasing.md) | [简体中文](releasing.md)

`main` 作为持续开发分支。功能分支及可选的 `dev` 分支执行相同的校验流程，完成的功能合入 `main`。分支推送不发布安装包；通过附注 Tag `vX.Y.Z` 选定发行的精确源码版本。

## 准备版本

以 `Cargo.toml` 中的产品版本为准，Rust 工作区各包继承该版本。`web/package.json`、`web/package-lock.json` 的两个根版本字段、`Cargo.lock` 和生成的 OpenAPI 版本必须一致。插件身份、存储 schema 和通信协议分别维护版本号。

修改 `Cargo.toml` 和 `web/package.json` 中的产品版本后，重新生成锁文件和 API：

```bash
cargo check --workspace --all-features
npm install --package-lock-only --ignore-scripts --prefix web
python3 tools/generate_api.py
python3 tools/render_api_docs.py
python3 tools/check_version.py
```

运行[开发检查](testing.md)，提交已完成的功能和版本调整，推送 `main`，等待 Verify 工作流通过。在首个稳定主版本前，兼容修复递增修订号，功能或契约变化递增次版本号。破坏性变化和升级步骤应写入发行说明。

## 推送 Tag

从已经审查、工作区干净的 `main` 执行。版本检查返回当前产品版本，避免 Tag 与实际产品版本不一致：

```bash
release_version=$(python3 tools/check_version.py)
git tag -a "v$release_version" -m "Release $release_version"
git push origin "v$release_version"
```

Release 工作流先核验 Tag 与各产品版本，调用完整 Verify 工作流，再使用固定 Rust 工具链与完整插件集，在原生 amd64、arm64 构建机上生成 musl 静态二进制。打包前检查二进制版本、静态 ELF 格式、公开内容边界和清单哈希。

两种架构都必须成功。CI 创建 Release 草稿，上传两个压缩包和 `SHA256SUMS`，再发布并自动生成发行说明。正式版本设为最新版本，带预发布标识的 Tag 保持为预发布。只有最终发布任务拥有仓库内容写权限，无需额外配置个人访问令牌。

## 下载与校验

[GitHub Releases 页面](https://github.com/LeeXN/converyn/releases)提供：

| 文件 | 内容 |
| --- | --- |
| `converyn-VERSION-linux-amd64.tar.gz` | amd64 静态二进制、安装器、初始化器、清单和许可证 |
| `converyn-VERSION-linux-arm64.tar.gz` | arm64 静态二进制、安装器、初始化器、清单和许可证 |
| `SHA256SUMS` | 两个压缩包的 SHA-256 校验值 |

下载两个压缩包后可校验完整校验文件；只下载一种架构时，选取该压缩包对应的校验行。解压目标架构的包，内含 `converyn-linux-amd64` 或 `converyn-linux-arm64`、`install.sh`、`init.sh`、`manifest.json` 和 `LICENSE`。同一包内的二进制与脚本应配套使用，后续步骤见[安装教程](../tutorials/quickstart.md)。

## 重试失败的发布

校验或构建失败不会发布 Release。源码问题在开发分支修复；若 Tag 中的源码正确，只是遇到临时故障，可重跑失败工作流，或以已有 Tag 为 ref 手动触发 Release：

```bash
gh workflow run release.yml --ref "v$release_version"
```

重试可以补齐已有草稿，并替换草稿中未上传完整的文件。已发布的 Release 及其 Tag 不覆盖；修正应使用新版本发布。不得移动已经发布的 Tag，也不得用同一版本号发布不同源码。排查失败时检查工作流记录与包内构建清单。[发行构建工具](../../../tools/build_release.py)也支持本地构建，见[贡献指南](../../../CONTRIBUTING.zh-CN.md)。
