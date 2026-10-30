# Installation and first run

[English](installation.md) | [简体中文](../../zh-CN/how-to/installation.md)

This guide covers all in one, one controller with multiple Agents, and 3/5-member HA. Commands assume a **fresh installation**; existing installations follow [operations](maintenance.md). `/secure` is a private directory on the operator machine. Replace example IPs, users and paths before running commands.

mTLS means mutual TLS: HTTPS encrypts the connection, the client verifies the controller certificate, and the controller also verifies the CLI/Agent client certificate. Tokens bind the individual identity; task grants restrict the permitted work. These checks share the existing HTTPS connection, with no additional bridge port or transport service.

Start with [installation to first deployment](../tutorials/quickstart.md), using `install.sh` and `init.sh` for deployment, personal identities and Nodes. Full script options are in the [installer](../reference/install-script.md) and [initializer](../reference/init-script.md) guides. The sections below are a manual installation reference; do not repeat bundle generation, membership initialization or first-OP creation already completed by the installer.

## 1. Prepare hosts and the binary

- Controllers and Agents run on Linux. Standard Agent installation needs systemd, `/bin/bash`, root and `useradd`. It creates a dedicated non-root `converyn-task` account. Controllers can use a separate service account.
- Build using the [root README](../../../README.md). Distribute the same build with the correct CPU architecture. `converyn get plugins --local` reports compiled capabilities.
- Give controllers stable IPs and local persistent disks; synchronize clocks. Members use `ID=IP:PORT`, not DNS names or wildcard addresses. The IP becomes the listener, advertised endpoint and certificate SAN.

| Direction | Example port | Purpose |
| --- | --- | --- |
| Operator/browser → controllers | TCP 8443 | CLI, GUI and API with mTLS |
| Agents → every controller | TCP 8443 | Enrollment, polling, grants, heartbeats and reports |
| Controller ↔ controller | TCP 8443 | Raft/internal traffic, sharing the API listener |
| Operator → Agent host (optional) | TCP 22 | SSH installation; normal execution does not use SSH |

Agents need no inbound execution port. Grant access to artifact sources and plugin destinations separately. Environments use separate names, CAs and state directories.

## 2. Choose a topology and generate bundles

Run on the operator machine. The output directory must not exist. Do not generate a second CA for the same installation.

Here manual `admin init` writes `/secure/converyn-qa`, directly containing `admin/`, `user-authority/` and `controller-N/`. Installer records in `/secure/converyn-install/qa` place these under `bundle/`, alongside `operator/` and checkpoints. `init.sh --installation` accepts the installer record root; manual bundles use explicit `--admin-config` and `--authority`, as shown in the walkthrough manual-install section.

**All in one:** both controller and Agent run locally, so loopback works:

```bash
install -d -m 0700 /secure
converyn admin init --directory /secure/converyn-qa --cluster qa \
  --member 1=127.0.0.1:8443
```

**One controller with remote Agents:** replace loopback with its actual reachable IP, for example `1=192.0.2.11:8443`. Remote Agents cannot use the operator/controller's loopback address.

**HA:** generate three members together and deploy each on a separate host. For five members, also provide `--member 4=... --member 5=...`.

```bash
install -d -m 0700 /secure
converyn admin init --directory /secure/converyn-qa --cluster qa \
  --member 1=192.0.2.11:8443 \
  --member 2=192.0.2.12:8443 \
  --member 3=192.0.2.13:8443
```

Supported topologies have 1/3/5 fixed members. `replace members` replaces members; it cannot expand a running single member into three. Three/five members need two/three available members to commit.

| Generated material | Destination and purpose |
| --- | --- |
| `controller-N/` | Corresponding host only: server profile, TLS, peer authentication and online Node issuer |
| `members.json` | Keep on operator machine for initialization |
| `admin/config.json` | Bootstrap identity for the first named administrator and defined recovery |
| `user-authority/` | Protect on the OP machine for personal issuance |
| `authority/`, `node-authority/` | Protect and back up CA material; never distribute to ordinary users or Agents |

Server profile paths resolve relative to `server.json`; `data_dir` defaults to `data`. Each member needs its own SQLite directory, never shared SQLite. Do not distribute the whole deployment bundle to workers.

## 3. Install and start controller services

For local installation use `controller-1` directly. For remote HA, verify SSH host keys and transfer the **corresponding bundle and binary**, for example:

```bash
scp -rp /secure/converyn-qa/controller-1 operator@192.0.2.11:~/
scp -p target/release/converyn operator@192.0.2.11:~/converyn
```

Run the following once on each controller. On remote hosts set `BUNDLE=$HOME/controller-1`; hosts two/three use their own directories. Install the transferred binary with `sudo install -m 0755 ~/converyn /usr/local/bin/converyn`.

```bash
BUNDLE=/secure/converyn-qa/controller-1
sudo useradd --system --user-group --no-create-home --shell /usr/sbin/nologin converyn-controller
sudo install -d -m 0700 /var/lib/converyn-controller
sudo cp -a "$BUNDLE/." /var/lib/converyn-controller/
sudo chown -R converyn-controller:converyn-controller /var/lib/converyn-controller
sudo chmod 0700 /var/lib/converyn-controller
sudo tee /etc/systemd/system/converyn-controller.service >/dev/null <<'UNIT'
[Unit]
Description=Converyn controller
Wants=network-online.target
After=network-online.target
[Service]
User=converyn-controller
Group=converyn-controller
WorkingDirectory=/var/lib/converyn-controller
ExecStart=/usr/local/bin/converyn server --config /var/lib/converyn-controller/server.json
Restart=on-failure
RestartSec=2
UMask=0077
[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
sudo systemctl enable --now converyn-controller
sudo journalctl -u converyn-controller -n 30 --no-pager
```

The example assumes a new service account; inspect an existing account before using it. For foreground evaluation use `converyn server --config /secure/converyn-qa/controller-1/server.json` instead of the service. Before initialization there is no Leader and readiness may fail.

## 4. Initialize once and create named people

After all controller processes start, run once on the original operator machine:

```bash
converyn admin initialize --config /secure/converyn-qa/admin/config.json \
  --members /secure/converyn-qa/members.json
converyn create identities --config /secure/converyn-qa/admin/config.json \
  --authority /secure/converyn-qa/user-authority --name OP --super-admin \
  --output /secure/qa-op
converyn config import /secure/qa-op/config.json --name qa-op --use
converyn --context qa-op create identities \
  --authority /secure/converyn-qa/user-authority --name Alice \
  --output /secure/qa-alice
converyn --context qa-op auth grant alice-demo --person Alice --role executor --workflow demo --preset qa
converyn config import /secure/qa-alice/config.json --name qa-alice
converyn --context qa-op status
converyn --context qa-op get members
```

Use named OP credentials for subsequent administration. Deliver only Alice's personal files to Alice over a private channel. Client directories require mode 0700 and files 0600. See [certificates](certificates.md) and [GUI](web-console.md) for rotation, issuer backups and browser access.

API config precedence is `--config` → `CONVERYN_CONFIG` → `~/.converyn/config.json`; `--context` selects a context within that file. First import selects it; later imports preserve selection unless `--use` is set. Replacing a name requires `--replace`. Server, Agent and `admin renew` require separate explicit service profiles.

## 5. Install Agents

Create a one-use bundle for each Node on the operator machine; default lifetime is 900 seconds:

```bash
converyn --context qa-op admin bootstrap --node node-a \
  --output /secure/node-a-bootstrap.json
```

**Local installation:** on the target host install as root. No node permission file is required:

```bash
sudo /usr/local/bin/converyn admin install-local --bootstrap /secure/node-a-bootstrap.json

```

Enrolled Agents trust approved workflows; no node policy file is required. Host scripts declare `execution: host`.

**Remote installation:** verify and record SSH host keys first. The command uses keys or an SSH agent; the remote account needs noninteractive sudo:

```bash
converyn admin install-ssh --host 192.0.2.21 --user operator \
  --identity-file /secure/ssh-key --known-hosts /secure/known_hosts \
  --binary target/release/converyn --bootstrap /secure/node-a-bootstrap.json
```

Enrolled Agents trust approved workflows; no node policy file is required. Host scripts declare `execution: host`.

On the host inspect `sudo converyn admin agent-info --state-dir /var/lib/converyn/state`; on the operator machine check a fresh heartbeat with `converyn --context qa-op get nodes node-a`.

## 6. Run and accept the example

Edit `example/workflow/inventory.yaml` with the enrolled UID, address and hostname, then run from the repository root:

```bash
converyn validate workflows --file example/workflow/workflow.yaml
converyn --context qa-op register workflows --file example/workflow/workflow.yaml
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --dry-run
converyn --context qa-alice run workflows demo --target qa --set VERSION=two --wait
converyn --context qa-alice get runs
```

Use the returned `RUN_ID` with `get runs RUN_ID`. Both file and script steps must have successful actual receipts. `logs runs RUN_ID --step verify --node node-a` should show the version, port and region. Check the actual `/opt/converyn-demo/config` on the Agent. A `--wait` timeout stops waiting only; continue tracking the same Run.

For HA verify business reads through every endpoint with the same identity, consistent membership/Leader observations, and majority/original Run recovery after stopping one controller during a disposable acceptance workflow. Service `active` alone is insufficient. A follower's local readiness may return 503; see [operations](maintenance.md).

Continue with [usage](runs.md) and [workflow/script authoring](workflows.md).
