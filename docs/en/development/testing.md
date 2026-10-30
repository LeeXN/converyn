# Build and test

[English](testing.md) | [简体中文](../../zh-CN/development/testing.md)

This page covers source builds, contract generation, public regressions and release verification. User installation and initialization belong to tutorials and script references; business scripts belong to workflow authoring.

## Directory ownership

| Location | Contents |
| --- | --- |
| `scripts/ops/` | Installation entry points and operator tools |
| `tools/` | Release builders, generators and contract checks |
| `scripts/ops/kubernetes/` | Public operator input tools |
| `scripts/tests/e2e/` | Public workflow, native outputs, pagination, package and browser acceptance |
| `scripts/tests/fixtures/`, `scripts/tests/support/` | Artifact/package fixtures and shared profile readers |

Keep new public tests in version control. Private scripts and dependencies are local maintenance assets, absent from a fresh checkout; public CI must not import them. Runtime credentials and inventory remain under `.local/env/`. Never ignore all `test_*.py` or `*_e2e.py` files.

## Setup and offline checks

Run from the repository root with the pinned Rust toolchain, a C compiler, Python and Node.js 24:

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

Build frontend assets first; Rust embeds `web/dist`. Python packages support development checks, not product runtime.

For contract edits, run the corresponding generators without `--check`, update API/frontend types or CLI references, then check again. See [documentation ownership](../README.md).

## Rust and frontend regressions

```bash
cargo test --workspace --all-features --locked -- --skip three_hundred_nodes_twenty_operators_and_large_history_baseline
```

As in CI, skip the capacity baseline on shared hardware; run that case explicitly on recorded dedicated hardware. Runtime tests need temporary files, loopback listeners and isolated processes. Enable ignored browser/package tests only with provisioned fixtures. Never run `package_live` against the host package database.

## Installer and initializer regressions

Top-level Python discovery does not recurse into `scripts/tests/init/` or `scripts/tests/install/` without package entry points. Run them explicitly as CI does:

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

Real mTLS fixtures use built `target/debug/converyn`, temporary directories and loopback listeners, replacing systemd/SSH installation actions without changing host services or existing labs. See [installer regressions](testing.md) and [initializer validation](../reference/init-script.md) for coverage.

## Current real workflow acceptance

After building full, run isolated temporary mTLS controllers/Agents:

```bash
python3 scripts/tests/e2e/workflow_e2e.py --binary target/debug/converyn \
  --output /tmp/converyn-workflow-result.json
python3 scripts/tests/e2e/workflow_e2e.py --binary target/debug/converyn \
  --controllers 3 --output /tmp/converyn-ha-workflow-result.json
```

This verifies actual file/script effects, personal audit, request deduplication, credential lifecycle and rejection of removed write APIs. HA terminates the Leader after the script writes a marker, then verifies the original Run succeeds with one effect. Existing VMs are not used. `--api-evidence /tmp/converyn-wire.jsonl` saves header-free HTTP shape evidence with mode 0600; validate using `.venv/bin/python tools/check_api_contract.py /tmp/converyn-wire.jsonl`.

Browser and pagination tests explicitly select Chrome; use fresh report paths:

```bash
python3 scripts/tests/e2e/workflow_e2e.py --binary target/debug/converyn \
  --chrome /usr/bin/google-chrome --browser-report /tmp/converyn-browser-result.json
python3 scripts/tests/e2e/pagination_e2e.py --chrome /usr/bin/google-chrome \
  --output /tmp/converyn-pagination-result.json
```

The browser relay supplies fixture client certificates to real HTTPS; it does not replace manual certificate-store import acceptance. CI's ignored Rust `browser` test invokes this same workflow fixture. See [Kubernetes regressions](testing.md) for offline generation/regression checks. Actual packages, reboots, Docker and Kubernetes need disposable dedicated environments.

## Release builds

`build_release.py` produces static binaries/provenance manifests; `plugin-build.rs` is a plugin build dependency. See [contributing](../../../CONTRIBUTING.md) for release builds and [operations](../how-to/maintenance.md) for deployed runtime evidence.

## Example tools and regression locations

[Operator tools](../how-to/kubernetes-tools.md) prepare inputs for native CLI deployment. [Kubernetes regressions](testing.md) hold mocked/offline acceptance. `example/` retains reference workflows, configuration, business scripts and the demonstration application.


## Real SSH and systemd enrollment regression

This opt-in test owns its container, temporary controller and credentials. It checks key SSH, sudo, real Agent installation, fresh heartbeats, and identity/certificate preservation on resume. It requires local Docker, a privileged systemd container, executable temporary storage and a matching static Linux binary. It never enrolls into an existing platform.

```bash
docker build -t converyn-ssh-fixture:local -f scripts/tests/fixtures/ssh/Dockerfile .
env CONVERYN_SSH_LIVE=1 CONVERYN_SYSTEMD_IMAGE=converyn-ssh-fixture:local \
  CONVERYN_TEST_BINARY="$PWD/release/converyn-linux-amd64" \
  python3 -m unittest discover -s scripts/tests/init -p 'test_ssh_enrollment.py' -v
python3 tools/check_public.py --release release
```
