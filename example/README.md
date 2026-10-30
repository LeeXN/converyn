# Runnable examples

[English](README.md) | [简体中文](README.zh-CN.md)

Complete the [installation tutorial](../docs/en/tutorials/quickstart.md), then choose an example. Replace Node UIDs, addresses and hostnames and prepare the platform node group. External Kubernetes uses its own target and connection.

| Example | Capabilities |
| --- | --- |
| [Quickstart](workflow/README.md) | Files, scripts, values |
| [Feature showcase](showcase/README.md) | Packages, artifacts, services, checks, outputs, Secrets |
| [Full Kubernetes](kubernetes/README.md) | Community cluster, addons and uninstall |
| [Linux CD](node-demo/README.md) | v1 → v2 → verification → v1 |
| [Kubernetes CD](external-kubernetes/README.md) | External API connection and versioned application release |
| [Reconciliation](reconciliation/README.md) | Detect and repair file drift |
| [Maintenance](maintenance/README.md) | Ordered reboot and recovery evidence |
| [Workflow directories](workflow-directory/README.md) | Ordered groups and shared scripts |
| [Outputs](outputs/README.md) | Protected typed dataflow |
| [Plugins](plugins/README.md) | Host and container plugins |
| [CI templates](ci/README.md) | GitHub Actions, GitLab CI, Jenkins |

## Common workflow

Prepare Kubernetes and showcase sources using their README first. Other workflow examples can be registered directly.

```bash
converyn validate workflows --file example/workflow
converyn --context qa-op register workflows --file example/workflow
converyn --context qa-alice run workflows demo --target qa --dry-run
converyn --context qa-alice run workflows demo --target qa --wait
```

Registration fixes a source snapshot; register again after edits. Check receipts and business results; preserve request IDs on network retries. Keep private inputs and generated material in ignored `.local/`. See the [coverage reference](../docs/en/reference/example-coverage.md).
