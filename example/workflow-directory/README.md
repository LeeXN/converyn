# Ordered node groups

[English](README.md) | [简体中文](README.zh-CN.md)

Run a small health script on two enrolled Nodes in an explicit order. Complete [the minimal workflow](../workflow/README.md) first; this example needs only ordinary unprivileged execution, with no business write grants.

## Prepare

Edit [inventory.yaml](inventory.yaml) with actual UIDs, addresses and hostnames. `database: [node-b, node-a]` defines execution order; `monitoring` demonstrates that the same Nodes can have several labels. Edit [values.yaml](values.yaml) to change the message.

## Deploy

```bash
converyn validate workflows --file example/workflow-directory
converyn --context qa-op register workflows --file example/workflow-directory
converyn --context qa-alice run workflows ordered-health --target default --dry-run
converyn --context qa-alice run workflows ordered-health --target default --wait
```

## Read and adapt

[workflow.yaml](workflow.yaml) selects `database` with `max_parallel: 1`: node-b completes before node-a starts. [scripts/check.sh](scripts/check.sh) reads [lib/message.sh](lib/message.sh) from the registered dependency package through `CONVERYN_PACKAGE_ROOT`. Registration packages the files; no separate copy to Agents is needed.

Inspect `converyn logs runs RUN_ID --step check` for both messages. Change the group to `database[0]` to select just its first Node. See [directory semantics](../../docs/en/how-to/workflow-directories.md) for selections, package limits and permissions.
