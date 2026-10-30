# Feature showcase

[English](README.md) | [简体中文](README.zh-CN.md)


One workflow demonstrates directories, a real deb/RPM, controller-distributed artifacts, file delivery, a systemd service, HTTP checks, scripts, typed outputs and a versioned Secret. Use an enrolled Linux/systemd test host, the full binary, Python 3 and curl. This example owns only its demonstration package, service and `/opt/converyn-showcase` directory. Node groups and workflow permissions are prepared by the platform administrator.

## Prepare and run

Create `/secure/showcase-token` privately (0600); do not commit its contents. The package has no installation scripts and writes one demonstration file. The preparation tool never installs packages. First follow the [quickstart](../../docs/en/tutorials/quickstart.md).

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

## Verify and clean up

Successful receipts must include package version, artifact digests, HTTP body and output validation. Verification checks Secret presence without printing its value. The service runs as nobody on loopback port 18081; its supervisor scripts do not require a legacy Agent policy fragment.

After stopping this workflow, remove `converyn-showcase.service`, its demo package and `/opt/converyn-showcase` using your host package manager and systemd. Do not delete Converyn state. Reboot and continuing reconciliation have separate [maintenance](../maintenance/README.md) and [reconciliation](../reconciliation/README.md) examples.
