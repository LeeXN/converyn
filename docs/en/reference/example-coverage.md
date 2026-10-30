# Example capability coverage

| Capability | Entry | Acceptance |
| --- | --- | --- |
| File, binary, package, service, HTTP, exec, plugin | [showcase](../../../example/showcase/README.md) | Managed bytes, exact package version and healthy service |
| Secret and typed outputs | [outputs](../../../example/outputs/README.md) | No plaintext in public results; valid consumer input |
| Ordered inventory and batching | [workflow-directory](../../../example/workflow-directory/README.md) | Correct ordering and independent workflow labels |
| Linux release and rollback | [node-demo](../../../example/node-demo/README.md) | v1 → v2 → v1 and HTTP digest |
| Kubernetes and community storage | [kubernetes](../../../example/kubernetes/README.md) | DNS, network, PVC, TLS, OCI and metrics |
| External Kubernetes CD | [external-kubernetes](../../../example/external-kubernetes/README.md) | Deployment ready and expected served version |
| Drift correction | [reconciliation](../../../example/reconciliation/README.md) | New system Run restores managed content |
| Reboot | [maintenance](../../../example/maintenance/README.md) | New boot ID and authenticated recovery |
| CI integration | [ci](../../../example/ci/README.md) | Reviewed templates; no deployment triggered by this repository |

[Identity/permissions](../how-to/access.md), [node groups](../how-to/node-groups.md), [credentials](../how-to/certificates.md), [web](../how-to/web-console.md) and [Run control/recovery](../how-to/runs.md) are exercised through focused guides. The capability tour does not reboot hosts, and CI templates do not provision platforms or credentials.
