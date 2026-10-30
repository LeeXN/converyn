# Step outputs

[English](README.md) | [简体中文](README.zh-CN.md)

The first Node produces a message and a random secret token. Every selected Node consumes the committed outputs. Complete [the minimal workflow](../workflow/README.md) first.

## Prepare

Edit [inventory.yaml](inventory.yaml) with enrolled Nodes. Ordinary unprivileged execution is sufficient. Controllers need the shared private `output_keyring` created by `admin init`; keep the same keyring on every controller.

## Deploy

```bash
converyn validate workflows --file example/outputs
converyn --context qa-op register workflows --file example/outputs
converyn --context qa-alice run workflows outputs-demo --target default --dry-run
converyn --context qa-alice run workflows outputs-demo --target default --wait
converyn --context qa-alice get runs RUN_ID
converyn --context qa-alice logs runs RUN_ID --step consume
```

## Read and adapt

[workflow.yaml](workflow.yaml) declares typed outputs, a one-hour token lifetime and explicit consumer bindings. [produce.sh](produce.sh) publishes through the installed Converyn binary's `task emit`; [consume.sh](consume.sh) verifies the received message and token. Add another Node to observe fan-out.

Use the returned ID in place of `RUN_ID`. Run details show the public message and a protected-output marker. Producer logs are suppressed and known secret values are redacted from consumer logs. See [output contracts](../../docs/en/how-to/outputs.md) for collection and recovery.
