# Linux application delivery

[English](README.md) | [简体中文](README.zh-CN.md)


Deliver an HTTP application and systemd unit to an enrolled Linux host with Python 3 and systemd. Edit inventory, prepare its node group and grant the executor access as in the [quickstart](../../docs/en/tutorials/quickstart.md). The workflow creates its own demonstration directories; it needs no Agent whitelist changes. The HTTP service runs as nobody, while reviewed deployment scripts declare host execution explicitly. The workflow explicitly restarts the service to load the delivered application code.

## Publish, upgrade and restore

```bash
converyn validate workflows --file example/node-demo
converyn --context qa-op register workflows --file example/node-demo
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v1 --dry-run
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v1 --wait
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v2 --wait
converyn --context qa-alice run workflows node-site-demo --target default --set VERSION=v1 --wait
converyn --context qa-alice logs runs RUN_ID --step verify
```

## Acceptance and cleanup

Each verification checks HTTP health, version and content SHA-256 on loopback port 18080. `publish.sh` preserves immutable `data/releases/VERSION` directories and atomically switches `data/current`. Conflicting bytes for an existing version are rejected. Restoring v1 preserves v2 for inspection.

After stopping use, stop and disable `converyn-node-demo.service`, remove its unit, reload systemd, and remove `/opt/converyn-node-demo` only when its release history is no longer needed. Converyn identities and state remain. CI templates are in [CI integration](../ci/README.md).
