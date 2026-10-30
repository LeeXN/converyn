# From installation to your first deployment

[English](quickstart.md) | [简体中文](../../zh-CN/tutorials/quickstart.md)

Start with a fresh instance and complete platform installation, personal certificates/contexts, Node enrollment and example execution. Examples use instance `qa` with records in `/secure/converyn-install/qa`. For an existing installation skip deployment and use its actual records and Node identities; see [operations](../how-to/maintenance.md) for upgrades.

## 1. Where files belong

Keep `install.sh`, [init.sh](../../../scripts/ops/init.sh) and a matching Converyn binary on the **operator machine**. Run from the repository root with release binary `./converyn-linux-amd64`; for source builds substitute `./target/release/converyn`. In a release directory use `./install.sh` and `./init.sh`; obtain workflow examples from the repository separately. Neither script needs Python/jq; service hosts require Linux, systemd, Bash 4.4+, GNU coreutils and flock.

| Machine | Files |
| --- | --- |
| Operator | Both management scripts, binary, workflow sources and private installation/issuer records |
| Controller | Its own controller-N configuration and running service, distributed by the installer |
| Agent | Its own Node delivery directory; no repository, OP configuration or CA private key |

The examples use `sudo` to read root-owned installation records. This affects filesystem access; the selected context still determines the API identity. Ordinary recipients import only their own configuration into `~/.converyn/config.json`.

## 2. Install the platform

In the wizard choose all-in-one, one controller with multiple Agents, or HA, and set the private record directory to `/secure/converyn-install/qa`. For this demo use Agent UID `node-a` without a node permission file:

```bash
sudo bash scripts/ops/install.sh --binary ./converyn-linux-amd64

```

The wizard generates bundles, deploys services, initializes membership and creates named OP. Selected Agents are checked for authenticated heartbeats. On completion it imports OP into the operator's default context store; it does not register business workflows/host groups. See the [installer guide](../reference/install-script.md) for HA/SSH and prepare/resume arguments.

Remote targets can use SSH and sudo passwords in an interactive terminal; select `password` in the wizard or `--ssh-auth password`, and omit `--non-interactive`. Keys remain supported. See the [installer authentication guide](../reference/install-script.md#password-based-remote-deployment).

The wizard saves validated non-secret defaults after each field and prefills them on the next run. Use arrows/Backspace to edit or press Enter to reuse them. Passwords and confirmation are not saved; details are in the [input-cache guide](../reference/install-script.md#wizard-defaults-and-editing).

Alternatively, this all-in-one example installs node-a with the minimal workflow's file policy. All clients are local; use a reachable stable IP for browsers or remote Agents:

```bash
sudo bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation /secure/converyn-install/qa \
  --member 1=127.0.0.1:8443 --controller 1=local --agent node-a=local \
  --yes --non-interactive
```

Choose either interactive or noninteractive deployment; do not deploy a fresh installation twice. The installer places binaries on destination hosts; after remote-only deployment, install the same build if the operator has no `converyn` command yet:

```bash
sudo install -m 0755 ./converyn-linux-amd64 /usr/local/bin/converyn
```

Check with the personal OP configuration created by the installer:

```bash
sudo converyn --config /secure/converyn-install/qa/operator/config.json status
sudo converyn --config /secure/converyn-install/qa/operator/config.json get members
```

## 3. Initialize personal certificates and contexts

Interactive wizard:

```bash
sudo bash scripts/ops/init.sh --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json
```
The installer normally imports OP into the default CLI config automatically. The explicit import below creates the separate example context file and reuses that same identity:

```bash
sudo bash scripts/ops/init.sh import --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json --context-name qa-op --use \
  --yes --non-interactive
sudo converyn --config /secure/converyn-install/qa/contexts.json config get-contexts
```
Prepare credential and Node output parent directories first; leave the individual credential directories for issuance to create:

```bash
sudo install -d -m 0700 /secure/converyn-users /secure/converyn-nodes
```

Issue **separate personal certificates** and contexts for an executor and a reader:

```bash
sudo bash scripts/ops/init.sh user --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json \
  --name Alice --context-name qa-alice --role executor --workflow demo --preset qa \
  --output /secure/converyn-users/qa-alice --yes --non-interactive
sudo bash scripts/ops/init.sh user --installation /secure/converyn-install/qa \
  --contexts /secure/converyn-install/qa/contexts.json \
  --name Reader --context-name qa-reader --role viewer --workflow demo \
  --output /secure/converyn-users/qa-reader --yes --non-interactive
```
A context is a local connection alias. Server-side bindings determine permissions; renaming a context cannot raise privileges. Alice can execute only `demo/qa`, while Reader can view `demo`.

| role | Purpose |
| --- | --- |
| `viewer` | View authorized workflows and runs |
| `executor` | Preview/execute authorized workflows and control own runs |
| `maintainer` | Maintain, execute and control a workflow across all presets |
| `platform_operator` | Manage Nodes, node groups and the control plane |
| `access_admin` | Manage identities, groups and access |
| `super_admin` | All instance capabilities |

Business roles use `--workflow ID`; viewers/executors may add `--preset NAME`. Global business grants require explicit `--all-workflows`. Platform roles do not automatically grant business execution. Personal roles are API permissions; workflow inventory and `hosts` select deployment nodes.

## 4. Add an Agent Node

If the installer already enrolled node-a, skip issuance and installation here and check directory and heartbeat. For a new, unenrolled node-a, prepare its delivery on the **operator**:

```bash
sudo bash scripts/ops/init.sh node --installation /secure/converyn-install/qa \
  --node node-a \
  --output /secure/converyn-nodes/node-a --yes --non-interactive
```
The directory contains `converyn`, `install.sh`, a one-use `bootstrap.json`, checksums and `INSTALL.txt`. OP credentials and issuing CAs are excluded. Bootstraps expire after one hour; the Node must match the binary architecture. Follow the generated `INSTALL.txt`. Nodes trust approved control-plane operations without local grant files. `--ttl-seconds` accepts 60–3600 seconds.

The SSH user on the **target Node** prepares a private directory:

```bash
install -d -m 0700 "$HOME/converyn-node"
```
On the **operator**, copy using independently verified SSH host keys; replace the address, username and known-hosts path:

```bash
sudo scp -rp -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile=/secure/known_hosts \
  /secure/converyn-nodes/node-a/. operator@192.0.2.21:converyn-node/
```
Install the Agent on the **target Node**:

```bash
cd "$HOME/converyn-node"
sha256sum --check checksums.sha256
sudo bash ./install.sh agent --binary ./converyn --bootstrap ./bootstrap.json \
  --yes --non-interactive
```
Back on the **operator**, verify its authenticated heartbeat:

```bash
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-op get nodes node-a
```
Enrolled Agents trust approved workflows; no node policy file is required. Host scripts declare `execution: host`.

Enrolled Agents trust approved workflows; no node policy file is required. Host scripts declare `execution: host`.

## 5. Where business scripts belong and how to deploy

Keep business scripts in the **workflow project on the operator**. `example/workflow/` contains `workflow.yaml`, `inventory.yaml`, `verify.sh`, `config.txt` and `values.yaml`. `scripts`/`files` paths are relative to the workflow file. Registration packages immutable definitions, which Agents download under grants; do not manually distribute every script.

Run from the repository root after editing `inventory.yaml` with the enrolled Node UID, address and hostname. Create the platform node group first, then register with both workflow maintenance and platform permissions. `qa` is a user-defined preset name; see [node groups and workflow scope](../how-to/node-groups.md):

```bash
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-op \
  update node-groups --file example/node-group.yaml
converyn validate workflows --file example/workflow/workflow.yaml
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-op \
  register workflows --file example/workflow/workflow.yaml
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice \
  run workflows demo --target qa --set VERSION=two --request-id demo-001 --dry-run
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice \
  run workflows demo --target qa --set VERSION=two --request-id demo-001 --wait
```
Read actual receipts and logs:

```bash
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice get runs run-demo-001
sudo converyn --config /secure/converyn-install/qa/contexts.json --context qa-alice logs runs run-demo-001 --follow
```

## 6. Deliver personal configuration and browser certificates

Privately deliver only `/secure/converyn-users/qa-alice/config.json` to Alice, never the contexts store containing OP/other identities. Alice saves it as mode 0600 and runs on her own machine:

```bash
chmod 600 ./alice-config.json
converyn config import ./alice-config.json --name qa-alice --use
converyn --context qa-alice auth status
converyn --context qa-alice get workflows
```
The operator can export Alice’s browser delivery, prompting for the P12 password:

```bash
sudo bash scripts/ops/init.sh browser --file /secure/converyn-users/qa-alice/config.json \
  --output /secure/converyn-users/qa-alice-browser
```
For noninteractive use add `--password-file /secure/private-password-file` (0600). Output contains `client.p12`, `token.txt`, CA and login instructions. P12 export requires OpenSSL on the operator. Personal PEM/P12 represent the same identity; roles remain access bindings.

## 7. Expansion, recovery and manual installation

For node-b repeat delivery/installation, append its real UID, address and hostname to `example/workflow/inventory.yaml`, then re-register the workflow. Add node-b to the approved node group as a platform operator. Enrollment or node-group membership alone does not add deployment nodes. Existing Runs retain their original inventory.

After issuance/delivery interruption, repeat the same request with `--resume`. Existing contexts are preserved unless `--replace-context` explicitly replaces that alias. Inspect expired/consumed bootstraps and existing Node identities rather than changing UIDs or deleting state.

For manual installations with no named administrator, start/initialize controllers first, then create the first OP:

```bash
bash scripts/ops/init.sh operator --admin-config /secure/converyn-qa/admin/config.json \
  --authority /secure/converyn-qa/user-authority \
  --name OP --context-name qa-op --output /secure/converyn-users/qa-op \
  --contexts /secure/converyn-qa/contexts.json --use --yes --non-interactive
```
See [installation](../how-to/installation.md), [personal certificates](../how-to/certificates.md), and the [initializer guide](../reference/init-script.md) for actions, options, recovery boundaries and common failures. Browser export does not resume/overwrite a directory; retain the original package and choose a new directory for another export.
