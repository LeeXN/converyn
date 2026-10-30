# Continuous reconciliation

[English](README.md) | [简体中文](README.zh-CN.md)


Edit inventory and prepare the platform node group as in the [quickstart](../../docs/en/tutorials/quickstart.md). The first successful Run fixes the desired file contents. Every 30 seconds Converyn checks that snapshot and repairs drift; inspect the associated system Run and receipts.

## Run and introduce drift

```bash
converyn validate workflows --file example/reconciliation
converyn --context qa-op register workflows --file example/reconciliation
converyn --context qa-op run workflows reconciliation-demo --target default --wait
# On an enrolled target host, intentionally change only the demonstration file:
sudo sh -c 'printf "changed\n" > /opt/converyn-reconciliation/config.txt'
# After one reconciliation period, inspect the host and the associated system Run:
cat /opt/converyn-reconciliation/config.txt
converyn --context qa-op get workflows reconciliation-demo
converyn --context qa-op get runs
```

## Acceptance and shutdown

The file must return to `managed configuration` and the corrective Run must succeed. Before repurposing the directory, cancel the original Run that created this generation to stop reconciliation. Its successful receipt remains; an already authorized check may finish. Setting `reconcile_seconds: null` or disabling the workflow alone does not terminate a previously frozen generation. Use the Run ID returned by the initial execution.

Only file, binary, service, HTTP checks and reconcilable plugins participate. Scripts, packages, reboots and once-only host hooks are excluded from automatic replay. See [workflow semantics](../../docs/en/how-to/workflows.md).

```bash
converyn --context qa-op cancel runs ORIGINAL_RUN_ID
converyn --context qa-op get workflows reconciliation-demo
```
