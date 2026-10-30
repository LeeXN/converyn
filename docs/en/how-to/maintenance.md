# Operations and recovery

[English](maintenance.md) | [简体中文](../../zh-CN/how-to/maintenance.md)

See [installation](installation.md), [daily usage](runs.md), [certificates](certificates.md) and [local execution policies](workflows.md) for those topics. This page owns diagnosis, storage, backup, upgrades and recovery.

## Runtime diagnosis

| Evidence | Establishes | Also verify |
| --- | --- | --- |
| systemd `active`, `/health/live` | Process life | Quorum, authenticated business reads and actual effects |
| `/health/ready` | Local majority-readiness assessment | Leader and member metrics; a follower may return 503 |
| `converyn status` | Quorum-confirmed business overview | Leader is a local observation, not a probe of every member |
| `get nodes NODE` | Cached inventory, resources and authenticated heartbeat | Collection time, stale/errors and actual receipts |
| Successful Run and original receipts | Reported execution outcome | Business acceptance and real target state |

Use `journalctl -u converyn-controller` and `journalctl -u converyn-agent` on their hosts. Local `admin agent-info --state-dir /var/lib/converyn/state` exposes identity, active intent and waiting reasons. The listener requires mTLS; do not disable verification to diagnose authentication.

Quorum loss stops new grants. Agents retain original intent/grant/outbox/receipts and report after reconnecting. Client timeout does not stop work, pause does not revoke existing grants, and cancellation is not rollback. Administrators inspect original uncertain Work before resolving it; commands live in usage.

Command audit records people, credentials, workflows and outcomes separately from raw stdout/stderr. Disabling centralized logs does not disable audit. Local `admin run-script` is an isolated lab entry and produces no controller command audit.

## Raw log storage

Generated controller profiles have no centralized log backend. A single controller can merge this into `server.json`:

```json
{"log_store":{"backend":"filesystem","directory":"/var/lib/converyn-logs","shared":false,"retention_days":30}}
```

Create the directory with read/write permission for the controller service account before restarting with the updated config. HA uses S3 or genuinely shared storage, with `shared: true` and matching verified storage identity on every controller. Identically named independent local directories are not shared storage.

S3 uses `endpoint`, `bucket`, `region`, `prefix`, `access_key_file`, `secret_key_file` and optional `session_token_file`; see [API types](../reference/api-types.md). Protected credential files remain outside replicated state.

Backend failure does not prevent process startup; uploads back off independently. Service startup therefore does not prove full logs are readable. Default reads may explicitly fall back to receipt tails; full reads/exports fail without complete storage. Follow uses byte cursors; `--follow -o json` emits NDJSON. Saved output is capped at 8 MiB per stream and retained for at least 30 days. Receipts, hashes and expiry/missing markers remain. Logical backups include manifests, not external raw bytes; back up that backend separately.

## Node observations

Inventory is collected at startup and checked every five minutes; resource samples update every twenty seconds. Inventory is stale after ten minutes and resources after sixty seconds. Missing, unsupported and failed fields are explicit rather than zero. Inventory/resources are capped at 64/16 KiB with truncation flags.

Serial numbers are inventory, not enrollment binding. Collection installs no software; NVIDIA uses a bounded call to an existing `nvidia-smi`. Online/offline uses a sixty-second heartbeat window and does not establish device health or workflow success.

## Backup and same-version recovery

```bash
converyn --context qa-op admin backup --output /secure/qa-backup.json
sha256sum /secure/qa-backup.json
```

Logical backups include replicated state and cold history. Separately back up controller config, CA/issuers, personal profiles, Agent identities/execution directories and the raw log backend. Protect private backup directories and permissions.

Before restoring, physically isolate the old control plane so two instances cannot execute on the same targets. Use a matching binary and fresh directory. `--old-control-plane-fenced` declares that isolation is **already complete**; it does not perform fencing:

```bash
converyn admin restore --backup /secure/qa-backup.json \
  --data-dir /var/lib/converyn-restored --node-id 1 --old-control-plane-fenced
```

Point the member's `data_dir` at the restored directory, prepare replacement quorum membership/credentials, then start and initialize. Review members, named identities, paused Runs, original receipts and ownership before removing maintenance and resuming grants. See [CLI](../reference/cli.md). Copying a live SQLite file is not a substitute for logical backup.

## Upgrades and instance replacement

Wait for no in-flight execution, back up, and retain every host's old binary/config/state/unit files. Compare protocol, schema, `plugin_manifest_sha256` and binary SHA-256 in release manifests; version strings alone are insufficient.

| Situation | Action |
| --- | --- |
| Compatible additive update | Follow release constraints: all controllers first, verify replication/business reads, then Agents |
| Different plugin manifests or incompatible protocol | Coordinate supervisor/controller stops and a unified switch; do not mix manifests |
| Unsupported legacy business schema | Fresh instance/directories; retain matching old binary/data, never point at the old database |

Afterwards verify actual `/proc/PID/exe` digests, member consistency, quorum business reads, fresh Agent heartbeats and original/new receipts. After new formats are written, do not install an older binary that cannot read them. Fix forward or isolate and restore a matching old backup.

An upgrade/recovery of the same instance preserves Agent identities and execution evidence. An independent replacement instead uses new config, credentials and enrollment directories: stop the old control plane, do not copy old Agent execution directories into a new identity, and review ownership explicitly. Retain original keys, databases, logs and backups.

### Coordinated binary upgrade: standard systemd installation

This updates the same instance without reinitializing or reenrolling nodes. Compare old/new manifest schemas first; a different schema needs its supported migration path before opening the database. Matching version strings do not establish matching builds. See [static builds](../../../CONTRIBUTING.md#static-musl-release-builds) for release preparation.

#### 1. Inventory and freeze work

On each host, record installed roles, executable/config paths and service states:

```bash
systemctl show converyn-controller -p LoadState -p ActiveState -p ExecStart -p WorkingDirectory
systemctl show converyn-agent -p LoadState -p ActiveState -p ExecStart -p WorkingDirectory
```

The standard binary is /usr/local/bin/converyn. Controller configuration/data reside under /var/lib/converyn-controller; Agent configuration is /etc/converyn/agent.json with default state under /var/lib/converyn/state. For custom installations use actual ExecStart, data_dir and state_dir values without moving state.

On the operator host, use a CLI matching the running build and the existing personal OP profile:

```bash
ADMIN_CONFIG=/secure/converyn-install/qa/operator/config.json
converyn --config "$ADMIN_CONFIG" get runs
converyn --config "$ADMIN_CONFIG" get workflows
converyn --config "$ADMIN_CONFIG" admin backup --output /secure/before-update.backup
sha256sum /secure/before-update.backup
```

Finish or review every nonterminal Run, including preparing, paused, unconfirmed cancellation and needs_attention. Never replay or clear unknown work merely to upgrade. Prevent new submissions during maintenance: record enabled workflows, temporarily disable them with disable workflows UID when necessary, and restore only originally enabled workflows after acceptance. Disabling does not revoke existing grants.

The logical-backup client verifies the internal payload. Also retain the complete file SHA-256, original installation record, CA/issuers and personal contexts. Backup output must be a new file; prepare /secure as a private writable directory first.

#### 2. Distribute and verify before stopping

Copy the same full build for each target architecture to $HOME/converyn.new on every host. Replace the following placeholder with binary_sha256 from the release manifest:

```bash
EXPECTED_SHA256='REPLACE_WITH_MANIFEST_BINARY_SHA256'
printf '%s  %s\n' "$EXPECTED_SHA256" "$HOME/converyn.new" | sha256sum --check
chmod 755 "$HOME/converyn.new"
"$HOME/converyn.new" -o json get plugins --local
```



#### 3. Coordinate stops and cold backups

For differing protocols or plugin manifests, stop all installed Agents before all controllers. Execute each command only on hosts with that role:

```bash
sudo systemctl stop converyn-agent
```

```bash
sudo systemctl stop converyn-controller
```

Confirm exited services and MainPID 0 before backing up each host. These Linux/GNU tar commands skip absent roles and create a unique private backup directory:

```bash
sudo bash <<'SH'
set -euo pipefail
umask 077
backup_dir=$(mktemp -d /root/converyn-upgrade.XXXXXXXX)
paths=(usr/local/bin/converyn)
for path in \
  etc/converyn \
  var/lib/converyn \
  var/lib/converyn-controller \
  etc/systemd/system/converyn-agent.service \
  etc/systemd/system/converyn-controller.service
do
  [[ ! -e /$path ]] || paths+=("$path")
done
tar -C / --xattrs --acls --selinux -czf "$backup_dir/state.tgz" "${paths[@]}"
tar -tzf "$backup_dir/state.tgz" >/dev/null
sha256sum "$backup_dir/state.tgz"
printf 'Private backup: %s\n' "$backup_dir"
SH
```

Back up custom data/Secret directories, hardware identity files and raw-log backends separately. Copying a live SQLite file is not a cold backup. Keep old and new services from running together during the switch.

#### 4. Replace atomically and review configuration

Run on each host in the same maintenance window. The command rechecks the installed temporary file, atomically replaces within the same filesystem and restores SELinux labels when available:

```bash
sudo bash -s -- "$HOME/converyn.new" "$EXPECTED_SHA256" <<'SH'
set -euo pipefail
source_binary=$1
expected_sha256=$2
[[ $expected_sha256 =~ ^[0-9a-f]{64}$ ]]
[[ -f $source_binary && ! -L /usr/local/bin/converyn ]]
install -o root -g root -m 0755 "$source_binary" /usr/local/bin/converyn.next
printf '%s  %s\n' "$expected_sha256" /usr/local/bin/converyn.next | sha256sum --check
mv -f /usr/local/bin/converyn.next /usr/local/bin/converyn
if command -v restorecon >/dev/null; then restorecon /usr/local/bin/converyn; fi
SH
```

Do not rerun installation wizards or use install.sh --resume as an upgrade. Issue no new bootstrap, and retain node_uid, cluster, TLS, tokens, hardware binding, state_dir and receipts. Binary replacement alone needs no daemon-reload.

Compare new get plugins --local descriptors. Re-register plugin workflows from native source rather than an exported definition with an old call identity. Nodes require no plugin grant files. For controller connections, review the corresponding plugin identity, allowed targets and connection credentials while preserving other existing configuration.

#### 5. Recover controllers before Agents

Start every controller first; do not wait for whole-cluster readiness after starting only the first member:

```bash
sudo systemctl start converyn-controller
```

```bash
converyn --config "$ADMIN_CONFIG" status
converyn --config "$ADMIN_CONFIG" get members
```

```bash
sudo systemctl start converyn-agent
```

Run the middle diagnostics on the operator host. Confirm leader/quorum business reads, original voters and addresses, and leader replication covering all members before starting Agents. Per-controller metrics are at /api/v1/controlplane/metrics; combine them with /health/live, /health/ready and authenticated business reads. A follower-local ready 503 does not mean the entire cluster is unavailable.

#### 6. Verify running executables and receipts

Check actual process hashes for installed roles rather than disk files alone. MainPID must be nonzero:

```bash
pid=$(systemctl show converyn-controller -p MainPID --value)
sudo sha256sum "/proc/$pid/exe"

pid=$(systemctl show converyn-agent -p MainPID --value)
sudo sha256sum "/proc/$pid/exe"

converyn --config "$ADMIN_CONFIG" get nodes
sudo journalctl -u converyn-controller -n 100 --no-pager
sudo journalctl -u converyn-agent -n 100 --no-pager
```

Every running digest must match the manifest. Agents should show new sessions and fresh heartbeats with unchanged UIDs, hardware digests, certificates and tokens. Verify historical identities, tasks and receipts remain readable. Execute a read-only acceptance workflow on target nodes, checking each success receipt and non-root execution identity, then disable the acceptance workflow, retain audit and restore original enabled workflows.

For later node additions explicitly select the new binary, for example --binary /usr/local/bin/converyn, with all three matching Shell scripts from the new release directory. Keep the original installation record as credential/recovery material.

#### Failure and rollback

Retain the maintenance window and backups while diagnosing journal/private evidence. Do not recreate identities or clear state. Once new models are written, replacing only the executable with an older build is unsafe. Stop and fence the new control plane, then restore matching old binaries, configuration and complete backups. Reverting only one controller or mixing protocols/manifests is not rollback.

## Audited history cleanup

Only when explicitly needed, export/verify a backup and read `run_sequence` from `get history`. An administrator submits this with `update history --file clear.json`:

```json
{
  "action": "clear_runs",
  "expected_sequence": 123,
  "backup_sha256": "<verified 64-character SHA256>",
  "evidence": "Explicit cleanup request and associated record"
}
```

The sequence must match and no running/unresolved work may exist. Cleanup removes hot/cold Runs and execution snapshots, retaining identities, workflows, ownership, fences, request tombstones, security/command audit and external backups. Cleared request IDs cannot replay. Log indexes remain until original output is reclaimed.

The output encryption keyring is separate from state backups. Retain old keys and distribute the same complete keyring to all controllers; see [output contracts](outputs.md) for rotation and recovery.


## Resume input failures before execution

Use `converyn resume runs RUN_ID` or the console's “Resume unstarted input steps” only when a Run failed during input materialization and every failed/canceled Work has no execution grant. Successful receipts and generated encrypted outputs remain frozen. Only never-started Work is requeued, with fresh node-group, occupancy and resource-owner admission. Executed failures, uncertain effects and a newer execution for the same owner cannot be replayed through this operation.
