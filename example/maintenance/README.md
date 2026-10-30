# Planned reboot

[English](README.md) | [简体中文](README.zh-CN.md)


This workflow really reboots each selected host. Use a test host or an approved maintenance window, update inventory, prepare the platform node group and stop conflicting business work first. Execute hosts one at a time; success requires a new boot ID, Agent recovery and a successful verification receipt.

## Run

```bash
converyn validate workflows --file example/maintenance
converyn --context qa-op register workflows --file example/maintenance
converyn --context qa-op run workflows maintenance-reboot --target default --dry-run
converyn --context qa-op run workflows maintenance-reboot --target default --request-id maintenance-001 --wait --timeout-seconds 900
converyn --context qa-op get runs run-maintenance-001
converyn --context qa-op logs runs run-maintenance-001
```

## Recovery

A timeout does not mean the host did not reboot. Inspect the original Run, console and Agent heartbeat; preserve its request ID when recovering a network error. Do not issue another reboot to hide an unknown outcome. This example writes no business data. See [Run control](../../docs/en/how-to/runs.md).
