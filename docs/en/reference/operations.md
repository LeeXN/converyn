# Built-in operations

[English](operations.md) | [简体中文](../../zh-CN/reference/operations.md)

Shell defaults to an unprivileged account; declare `execution: host` for host changes. Native files, packages, services and plugins use the approved operation declaration.

## Choose an operation

| kind | Purpose | Configuration/dependencies |
| --- | --- | --- |
| `file` | Fixed text bytes, digest, and mode | Approved host operation; no node allowlist |
| `binary` | Download a digest-pinned artifact and publish a path | Controller artifact or external `artifact_sources` |
| `package` | Exact deb/rpm version from a pinned artifact | Approved host operation; no node allowlist |
| `service` | Desired systemd running/enabled state | Approved host operation; no node allowlist |
| `http_check` | HTTP status and optional body digest from a Node | Approved host operation; no node allowlist |
| `exec` | Bash for work without a native equivalent | `scripts` mapping; user by default, explicit `execution: host` |
| `reboot` | Restart and wait for a changed boot ID and recovery | Approved host operation; no node allowlist |
| `plugin` | Compiled extension | Fixed call; approved Node operation or controller connection |

Enrolled nodes execute approved operations without per-node grants. Configure `artifact_sources` only for artifacts downloaded from an external server; controller-managed artifacts use the Node identity. Preserve the existing Agent identity, TLS and state when adding an external source:
```json
{
  "artifact_sources": {
    "sources": {
      "releases": {
        "origin": "https://artifacts.example.test",
        "ca_file": "/etc/converyn/artifact-ca.crt",
        "bearer_token_file": "/etc/converyn/artifact-token"
      }
    }
  }
}
```

Sources must be HTTPS origins. Omit CA/token fields only when using system trust or unauthenticated sources. Protect endpoint credentials. Restart an idle Agent after local configuration changes and confirm heartbeats.

## file: managed text

Set top-level `files: {config: config.json}` and add this step. Packaging computes `sha256`. The parent `/opt/myapp` must exist. Modes are decimal integers: 420 = 0644, 384 = 0600, 493 = 0755. Bytes are published without variable substitution, service creation, or implicit restarts.

```yaml
- id: config
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: file
    path: /opt/myapp/config.json
    mode: 420
```

## binary: artifact publication

Replace the illustrative digest and byte size with the reviewed artifact's actual values (`sha256sum app`, `wc -c < app`). `releases` resolves through each Agent's local sources; URL = origin + path. Artifacts are not inlined with `files` and are not automatically unpacked. Optional `version_directory` retains digest/mode versions and switches the publication path; prepare that directory first.

```yaml
- id: publish
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: binary
    path: /opt/myapp/bin/app
    version_directory: /opt/myapp/versions
    mode: 493
    artifact:
      sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      size: 12345
      locations:
        - source: releases
          path: /myapp/app-linux-amd64
```

## package: exact package version

Replace name, version, digest, size, and path with an actual deb. Managers are `deb` or `rpm` and require corresponding tools on the Agent. Installation uses the specified artifact, not `latest` or arbitrary repository resolution. `install_only: true` is available for RPM multiversion packages only. Package scripts can have side effects; unknown outcomes require investigation.

```yaml
- id: install
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: package
    manager: deb
    name: myapp
    version: "1.2.3-1"
    artifact:
      sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      size: 12345
      locations:
        - source: releases
          path: /myapp/myapp_1.2.3-1_amd64.deb
    timeout_seconds: 300
```

## service: desired service state

The service unit must already be installed. This operation verifies its running and enabled state. After updating a unit file, explicitly arrange reloading or restarting as required by the application.

```yaml
- id: start
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: service
    name: myapp.service
    running: true
    enabled: true
```

## http_check: verify HTTP results

Request the workflow URL from the executing Node and check its response status. An optional response digest verifies the actual body bytes.

```yaml
- id: health
  phase: verify
  scope: per_node
  hosts: worker
  operation:
    kind: http_check
    url: https://app.example.test/health
    expected_status: 200
    timeout_seconds: 10
```

## exec: custom scripts

Provide `scripts/verify.sh` inside the workflow package. Shell defaults to an unprivileged account; declare `execution: host` for host changes. Both retain timeouts, cancellation and receipts.

```yaml
- id: verify
  phase: verify
  scope: per_node
  hosts: worker
  operation:
    kind: exec
    script: scripts/verify.sh
    timeout_seconds: 30
    environment:
      EXPECTED_VERSION: "1.2.3"
```

## reboot: restart and recover

This operation restarts the host. Declare it explicitly in the workflow and schedule planned maintenance. Recovery timeout is 1–86400 seconds; optional `expected_kernel` fixes the expected kernel release. Success requires a changed boot ID and recovery evidence. Follow it with application health checks.

```yaml
- id: reboot
  phase: rollout
  scope: per_node
  hosts: worker
  operation:
    kind: reboot
    recovery_timeout_seconds: 600
```

## Compose, preview, and diagnose

A typical flow is prepare preflight → rollout configuration/artifact/service → verify health. Combine needed steps in a workflow, validate, register, inspect `run --dry-run`, then execute with `--wait`. Verify package/reboot operations on disposable targets first.

| Symptom | Check |
| --- | --- |
| Denied file path/missing directory | Parent directories, protected paths and current task grant |
| Shell cannot write where file can | Approved host operation; no node allowlist |
| Missing source/digest mismatch | Every Agent's artifact sources; actual SHA-256 and byte size |
| Denied HTTP origin | Workflow URL, TLS configuration and actual port |
| Service succeeds but application is old | Running executable, configuration loading, and application API |
| Offline/waiting for reboot | Original Run, heartbeat, boot ID, kernel, and recovery timeout |
| Missing/denied plugin | Approved host operation; no node allowlist |

Generate `plugin` operations with `compile plugins -o json`. Calls pin plugin identity, action, execution location, and business spec; do not invent digests. See [plugins](../how-to/plugins.md) for host operations and [external Kubernetes](../how-to/external-kubernetes.md) for controller connections. Complete fields are in [API types](api-types.md).
