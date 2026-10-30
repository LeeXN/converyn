# Deploy through host plugins

[English](README.md) | [简体中文](README.zh-CN.md)

Enrolled nodes execute approved plugin operations without local grant files.

## Sysctl

[workflow.yaml](workflow.yaml) directly declares `converyn.linux-host / sysctl`; [sysctl-spec.json](sysctl-spec.json) enables persistent IP forwarding on the selected test Nodes. No operation/workflow generation is needed. Validate the workflow and register it with matching controller and Agent builds:

```bash
converyn --context qa-op register workflows --file example/plugins
converyn --context qa-alice run workflows host-sysctl-demo --target default --dry-run
converyn --context qa-alice run workflows host-sysctl-demo --target default --wait
```

On the Node verify `sysctl net.ipv4.ip_forward` and the persistent file; inspect the plugin receipt in `get runs RUN_ID`.

## Docker

Edit [docker-spec.yaml](docker-spec.yaml): it pins the same BusyBox image as the Kubernetes verification example. Choose the actual platform; pin any replacement image by digest and retain sh/httpd/wget. Nodes need Docker Engine/CLI and a root Agent supervisor. [docker.yaml](docker.yaml) is another complete workflow using the same inventory.

```bash
converyn --context qa-op register workflows --file example/plugins/docker.yaml
converyn --context qa-alice run workflows docker-web-demo --target default --dry-run
converyn --context qa-alice run workflows docker-web-demo --target default --wait
```

The container exposes `127.0.0.1:8080`; its healthcheck must pass. Container name, registry, platform, digest and approved task grant are verified during execution.

## Further reading

[kubeadm-check-input.yaml](kubeadm-check-input.yaml) is an optional expansion input for learning `expand plugins`, covered in [the plugin guide](../../docs/en/how-to/plugins.md); use [Kubernetes foundation](../kubernetes/README.md) for a complete installation workflow. [External Kubernetes](../external-kubernetes/README.md) runs at controllers. [plugins/example](../../plugins/example) is the SDK development reference.
