# Minimal workflow

[English](README.md) | [简体中文](README.zh-CN.md)

Write one configuration file, then verify it on an enrolled Node. Start here after [installation and enrollment](../../docs/en/tutorials/quickstart.md). Commands run from the repository root; `qa-op` registers the workflow and `qa-alice` needs executor permission for `demo`.

## Prepare

Edit [inventory.yaml](inventory.yaml) with the actual Node UID, address and hostname. The workflow uses its own inventory; create the bound [platform node group](../../docs/en/how-to/node-groups.md) first. If the UID changes, update the `node_values` key in [workflow.yaml](workflow.yaml) too.

Enrolled Agents execute approved native file operations without editing local permissions.

## Deploy

```bash
converyn validate workflows --file example/workflow
converyn --context qa-op register workflows --file example/workflow
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --dry-run
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --wait
```

## Read and adapt

| File | What to learn |
| --- | --- |
| [workflow.yaml](workflow.yaml) | Target, file delivery, verification and per-node defaults |
| [values.yaml](values.yaml) | Default values; `VERSION` can change per run, `REGION` is immutable |
| [config.txt](config.txt) | Fixed content delivered to `/opt/converyn-demo/config` |
| [verify.sh](verify.sh) | Verify the real file and print the resolved values |

The file contents stay fixed when `VERSION` changes: values reach the script environment. Re-register after editing source files. Inspect `converyn get runs RUN_ID` and `converyn logs runs RUN_ID --step verify`; both operations need successful receipts. Continue with [ordered groups](../workflow-directory/README.md) or [site deployment](../node-demo/README.md).
