# Contributing

[English](CONTRIBUTING.md) | [简体中文](CONTRIBUTING.zh-CN.md)

Bug reports, documentation fixes, and focused pull requests are welcome. For a larger change, open an issue describing the problem and proposed behavior first.

## Current-model policy

Converyn is in initial development. Maintain one current workflow, configuration and CLI contract; do not add compatibility-only branches or aliases. Update callers, examples and tests with contract changes, document breaking changes and upgrade steps, and preserve credentials, node identities and execution evidence during updates.

## Development setup

Use Linux and the toolchain in `rust-toolchain.toml`. Install a C toolchain for bundled native dependencies. Build console assets with `npm ci --prefix web` and `npm run build --prefix web` before Rust compilation; `web/dist` is generated and ignored.

```bash
npm ci --prefix web
npm run build --prefix web
cargo build -p converyn --locked --features full
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo test --workspace --all-features --locked -- --skip three_hundred_nodes_twenty_operators_and_large_history_baseline
```

For console changes:

```bash
cd web
npm ci
npm run build
```

Integration tests marked `ignored` require provisioned Kubernetes, package-manager or browser fixtures. Package installation and reboot tests run on dedicated test hosts. Prerequisites and commands are listed in [test tools](docs/en/development/testing.md).

## Editor setup

The pinned toolchain includes `rust-src` for rust-analyzer. Run `rustup show` from the repository to install and select it. In VS Code, run **rust-analyzer: Restart server** after a toolchain change; use **Developer: Reload Window** if the extension still reports the previous version.

`rust-toolchain.toml` selects the development compiler. The `rust-version` field in `Cargo.toml` declares the minimum supported Rust version and does not select the active toolchain.

## Project structure

| Directory | Responsibility |
| --- | --- |
| `crates/plugin-api` | Plugin contracts and host capability interfaces |
| `crates/core` | Domain model, plans, and deterministic state transitions |
| `crates/runtime` | Controllers, agents, persistence, credentials, and execution |
| `crates/cli` | Command-line interface and binary assembly |
| `plugins` | Optional plugins and embedded script packages |
| `web` | Embedded console |
| `example` | Sample definitions and deployment flows |
| scripts | Installation, initialization, operator helpers, build and tests |

Keep external I/O out of the core state machine. Scenario code belongs in plugins; see [Plugin development](docs/en/how-to/plugins.md). Changes to execution or recovery must preserve immutable plans, authorization fences, original receipts, and unknown-result handling.

## Static musl release builds

On a Linux x86_64 **build host**, run these commands from the repository root. You need Python 3, Node.js 24/npm, `readelf` (binutils), and a C toolchain providing `musl-gcc`. For example, Debian provides `build-essential musl-tools binutils python3`; package names differ between distributions. Installing a Rust target alone does not install the C compiler; see the [Rustup cross-compilation guide](https://rust-lang.github.io/rustup/cross-compilation.html).

```bash
rustup show
rustup target add x86_64-unknown-linux-musl
command -v musl-gcc
command -v readelf

# Build web/dist first; Rust embeds its HTML, JS and CSS during compilation.
npm ci --prefix web
npm run build --prefix web

# The output directory must not exist; the default feature set is full.
python3 tools/build_release.py --arch amd64 \
  --output release/linux-amd64 --build-tag my-amd64-build
```



```bash
sha256sum release/linux-amd64/converyn-linux-amd64 \

cat release/linux-amd64/manifest.json
readelf -l release/linux-amd64/converyn-linux-amd64 | grep INTERP || echo 'No dynamic interpreter (static)'
```

Compare the values to `binary_sha256`, `installer_sha256`, and `initializer_sha256` in the manifest. Identify each build by its manifest and SHA-256, keep binaries and scripts together, and never replace a published version with different source. The builder refuses to overwrite an existing output directory.


To compile only the binary after building the frontend:

```bash
cargo build -p converyn --release --locked --no-default-features \
  --features full --target x86_64-unknown-linux-musl
# Binary: target/x86_64-unknown-linux-musl/release/converyn
```

This Cargo command does not create the release manifest or installer scripts; prefer `build_release.py` for coordinated deployments. ARM64 builds require the Rust target, Zig and readelf, plus an explicit emulator when the build host cannot run ARM64:

```bash
rustup target add aarch64-unknown-linux-musl
python3 tools/build_release.py --arch arm64 --zig /path/to/zig \
  --runner /path/to/qemu-aarch64-static --output release/linux-arm64
```

Build output excludes repository `example/`; obtain first-workflow examples separately. See the [installer](docs/en/reference/install-script.md) and [initializer](docs/en/reference/init-script.md) guides.

## Pull requests

Explain the problem, the resulting behavior, and how you verified the change. Include regression coverage for changes to state transitions, authorization, or recovery. Keep generated credentials, local data, build outputs, and environment-specific reports out of commits.

By submitting a contribution, you agree that it is provided under the project's [MIT License](LICENSE).

## API and console contracts

After changing a public route or Serde wire type, run
`python3 tools/generate_api.py`, `python3 tools/render_api_docs.py`, and rebuild
`web` before compiling the embedded binary. CI checks
`python3 tools/generate_api.py --check`. With the development `jsonschema`
package installed, `tools/check_api_contract.py` also validates the schemas and
optional JSONL evidence captured from the real HTTP fixture. It never records
Bearer headers. Browser regression requires an explicitly selected Chrome binary;
see [test tools](docs/en/development/testing.md).

The GitHub Actions verification workflow runs contract/document drift checks, the
console build, formatting, Clippy, runtime HTTP regressions and an explicit Chrome
fixture. Install Python validation dependencies with
`python3 -m pip install -r tools/requirements-dev.txt`. Shared hosted runners skip
the capacity SLO fixture; run it explicitly on recorded hardware before release.

## Documentation

Maintain English and Simplified Chinese versions together. Describe the feature, prerequisites, inputs, commands, outputs and error handling. Use concise, task-oriented instructions and reference tables. See [test tools](docs/en/development/testing.md) for contract/CLI generation and checks.

Documentation ownership is listed in the [documentation index](docs/en/README.md). Maintain installation, usage and authoring instructions at their canonical entry points. Update generated API and CLI references through their generators. Examples use documentation addresses and placeholders; private configuration and runtime records belong in `.local/`.

## Version releases

Use `main` for ongoing development and annotated version tags for releases. CI builds and uploads both Linux architectures automatically; follow [the release guide](docs/en/development/releasing.md).
