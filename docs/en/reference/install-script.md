# Interactive Shell installer

[English](install-script.md) | [简体中文](../../zh-CN/reference/install-script.md)

The entry point is [scripts/ops/install.sh](../../../scripts/ops/install.sh). Runtime dependencies are Bash, system commands and the Converyn binary, without Python, jq, Rust or Node.js. It supports fresh installations and recovery of the same installation record. One operator host can deploy 1/3/5 controllers and multiple Agents over verified SSH, or run the local interactive wizard.

## Shared Shell interface

The two Shell entry points share `--binary`, `--installation`, `--yes`, `--non-interactive` and `--help`. Installation records use `--installation` for creation and later reading; `init.sh` use `--output` for a new delivery directory. SSH selectors are `--ssh-host`, `--ssh-user`, `--ssh-key`, `--ssh-port` (22) and `--ssh-known-hosts` (`~/.ssh/known_hosts`); deploy uses topology selectors instead of a single `--ssh-host`. Underlying `converyn admin` CLI flags retain their native names.

No arguments with a terminal opens the wizard. Explicit actions can prompt for missing required inputs. `--yes` accepts the plan; `--non-interactive` forbids prompts, so unattended execution uses both. Options for another action are rejected. Binary discovery checks an explicit path, PATH, the adjacent release and then repository build outputs; an invalid explicit binary is never replaced by a fallback.

Progress, diagnostics and underlying command output go to stderr. After success, stdout contains one text summary with `Action`, `Result`, relevant record/output/config paths and the next command. Failure leaves stdout empty. Exit codes are 0 for success/help, 2 for invalid/missing inputs, and 1 for execution failure or cancellation. Cancellation precedes persistent installation records, issuance and host installation. Existing private records retain their format and checksum checks; recovery still requires the original frozen installer and binary.


## Interactive installation

```bash
sudo bash scripts/ops/install.sh --binary ./converyn-linux-amd64
```
Choose local all-in-one, one controller with multiple Agents, HA, Agent-only or CLI-only. The wizard asks for stable IPs, ports, Node UIDs, SSH targets and a private record directory, then shows the plan. Service hosts require Linux, systemd, Bash 4.4+, GNU coreutils, flock and root access. The operator needs curl; remote deployment also needs OpenSSH/scp/tar. Automatic firewalld configuration also needs `firewall-cmd` and `ip` on controllers. Non-root targets need sudo permission; interactive deployment accepts a sudo password when passwordless sudo is unavailable.

Use a full release binary with the same architecture on the operator and all targets. Release output includes `install.sh`, which discovers a matching `converyn-linux-amd64` or `converyn-linux-arm64` beside it; a file can also be selected explicitly. Without a binary, the wizard accepts a reviewed HTTPS URL and an independently verified SHA-256. It does not select latest or disable TLS/SSH verification. Controller firewall ports are configured as described below. The selected binary must match the installer’s bundle contract; the same version string alone does not prove compatibility.

## Wizard defaults and editing

Cluster name, administrator name, SSH username and Agent UID have no built-in identity defaults. Enter them explicitly or reuse values you previously saved in your own cache. Noninteractive deployment requires `--operator-name`; derived SSH controller targets also require `--ssh-user`. Names and paths in the examples are placeholders.

Each controller needs a nonempty valid IPv4/IPv6 address and a port from 1 to 65535. Enter the advertised IP in the IP field; the separate installation target defaults to `SSH-user@that-IP`. In the wizard, bare IPs or hostnames are expanded using the selected SSH user. CLI `--controller`/`--agent` targets still use `local` or `user@host`. Invalid input, repeated controller addresses/hosts and repeated Agent UIDs/hosts are rejected at that field, without restarting the wizard. One controller and one Agent may share a host.

Non-secret choices are saved immediately after validation to `${XDG_STATE_HOME:-$HOME/.local/state}/converyn/installer/inputs.tsv` (0600). On the next invocation, fields are prefilled; press Enter to accept them or edit with arrows and Backspace. Both BS and DEL Backspace encodings work; Ctrl+U clears a field. Required fields cannot be blank. The installer's hidden sudo-password input supports both Backspace encodings too. Passwords, tokens, download URLs and installation confirmations are never cached. The file is parsed as whitelisted data, never sourced or evaluated as shell code.

Use `--input-cache /absolute/path/inputs.tsv` for a separate set of defaults, or `--no-input-cache` to disable reading/writing them. Explicit CLI choices take precedence over cached defaults. The cache belongs to the account running the script; running with sudo uses root's cache. The installation-record default is `$HOME/.local/share/converyn-install/CLUSTER` for the account running the script. Unwritable or existing record directories are caught while entering the path.

The input cache is separate from the installation record. It preserves accepted input after interruption or validation failure, but never enables `--resume`, bypasses confirmation or replaces existing credentials/state. To resume an installation, retain the original record and frozen installer/binary and explicitly use `--resume`. Deleting the input-cache file clears only wizard defaults.

## Host firewall and SELinux

Controller installation defaults to `--firewall auto`. It reads the actual listen address/port from the controller profile. For active firewalld, it opens that TCP port in the listen interface's zone, both at runtime and permanently, without reloading other rules. For active UFW it adds the TCP-port rule. Loopback-only controllers need no inbound rule; Agents need no Converyn inbound port. Unmanaged firewall policies remain the administrator's responsibility; use `--firewall skip` when rules are managed externally.

After services start, the installer checks TCP access to every controller from the operator and all target hosts. It then confirms leader/quorum readiness before creating the first personal credential. Network failures identify the source and destination, retain the original record and stop before issuing an operator. A pending `operator/operation.json` is resumed rather than replaced.

SELinux defaults to `--selinux keep`. The explicit `--selinux permissive` option calls `setenforce 0` only when enforcing, for the current boot; it does not set persistent disabled mode or reboot a host. Firewall/SELinux choices are shown in the plan and propagated to remote host installation.

## Password-based remote deployment

No SSH keys or passwordless sudo are required for interactive deployment. Use a terminal and select `password` in the wizard, or provide `--ssh-auth password`:

```bash
bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation "$HOME/.local/share/converyn-install/qa" \
  --member 1=192.0.2.11:8443 --member 2=192.0.2.12:8443 --member 3=192.0.2.13:8443 \
  --controller 1=operator@192.0.2.11 --controller 2=operator@192.0.2.12 \
  --controller 3=operator@192.0.2.13 --agent node-a=operator@192.0.2.21 \
  --ssh-auth password --ssh-known-hosts "$HOME/.ssh/known_hosts"
```

OpenSSH asks for each target's SSH password. If that account needs a sudo password, the installer prompts separately without echo; the two passwords may differ. SSH/scp reuse a private connection socket, and sudo passwords stay only in the current process's memory and travel over SSH standard input. The privileged installer receives no password on its standard input. Passwords are not accepted as command-line values, exported in environment variables or written into installation records, staging or logs. The installer closes its SSH masters and removes sockets on exit; expired connections may require authentication again. Resume prompts again and reuses the original cluster identities.

`--ssh-auth auto` (default) allows keys/ssh-agent and interactive password fallback. `--ssh-auth key` requires usable keys/ssh-agent, but can still prompt for a sudo password in interactive mode. `--ssh-auth password` selects SSH password/keyboard-interactive authentication and cannot be combined with `--ssh-key`. It requires a terminal and cannot be used with `--non-interactive`. `--yes` accepts the installation plan and still allows authentication prompts. Fully noninteractive deployment continues to require SSH keys/agent and passwordless sudo.

Select a writable `--ssh-known-hosts` file. On first contact, interactive OpenSSH displays the host fingerprint; compare it with a trusted source and type `yes` to save it. A missing file is created with mode 0600. Changed host keys are rejected. Noninteractive runs require an existing file with verified keys and cannot prompt. Only the operator needs SSH access to the targets; controllers and Agents do not need mutual SSH logins for normal operation. A service installed on the operator still requires local sudo. The installer does not change SSH or sudo policies on target hosts.

## Noninteractive deployment

Local all-in-one installation:

```bash
sudo bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation /secure/converyn-install/qa \
  --member 1=127.0.0.1:8443 --controller 1=local --agent node-a=local \
  --yes --non-interactive
```
Loopback is for same-host clients only. Select an accessible stable IP when Agents, CLI users or browsers connect remotely.

Three controllers and two Agents:

```bash
bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation "$HOME/.local/share/converyn-install/qa" \
  --member 1=192.0.2.11:8443 --member 2=192.0.2.12:8443 --member 3=192.0.2.13:8443 \
  --controller 1=operator@192.0.2.11 --controller 2=operator@192.0.2.12 \
  --controller 3=operator@192.0.2.13 \
  --agent node-a=operator@192.0.2.21 --agent node-b=operator@192.0.2.22 \
  --ssh-key "$HOME/.ssh/id_ed25519" --ssh-known-hosts "$HOME/.ssh/known_hosts" \
  --yes --non-interactive
```
For this noninteractive example, independently verify and record SSH host keys in `known_hosts` before running it. A host may run one controller and one Agent, but multiple controllers cannot share it. Controllers receive only their own `controller-N` directory, not offline CA/personal-issuer or OP private keys. Agents receive their own short-lived one-use bootstrap.

Installation verifies/freezes the binary and script, generates private bundles, preflights hosts, installs systemd services, initializes the original membership, creates a named super administrator, checks every HTTPS entry with mTLS business reads, then installs Agents and requires authenticated heartbeats within 30 seconds. `systemctl active` alone is insufficient.

## Bundle preparation and individual hosts

```bash
bash scripts/ops/install.sh deploy --binary ./converyn-linux-amd64 \
  --cluster qa --operator-name OP --installation "$HOME/.local/share/converyn-install/qa" \
  --member 1=192.0.2.11:8443 --member 2=192.0.2.12:8443 --member 3=192.0.2.13:8443 \
  --ssh-user operator --prepare-only --yes --non-interactive
```
`--prepare-only` creates configuration, signing material and installation records without SSH connections or service changes. Resume with the same cluster, membership, targets, Agent UIDs, binary and policy; SSH authentication mode/key/port/known-hosts selectors may be updated explicitly; interactive passwords are requested again. Alternatively, privately transfer the matching controller directory and binary, then run:

```bash
sudo bash install.sh controller --binary ./converyn-linux-amd64 \
  --bundle ./controller-2 --yes --non-interactive
```
Create a one-use bootstrap for an additional Agent, privately transfer it to that node, then run the second command there:

```bash
converyn --context qa-op admin bootstrap --node node-c --output /secure/node-c-bootstrap.json
sudo bash scripts/ops/install.sh agent --binary ./converyn-linux-amd64 \
  --bootstrap /secure/node-c-bootstrap.json --yes --non-interactive
```
Enrolled Agents trust approved workflows; no node policy file is required. Host scripts declare `execution: host`.

CLI-only installation also accepts an explicit HTTPS raw binary URL; replace the digest placeholder:

```bash
sudo bash scripts/ops/install.sh client \
  --binary-url https://release.example.test/converyn-linux-amd64 \
  --sha256 REVIEWED_64_HEX_SHA256 --yes --non-interactive
```
## After deployment

A completed `deploy` imports the installer-created personal OP into the operator's default CLI config. The context name is the supplied cluster name. The file is selected by `CONVERYN_CONFIG`, or defaults to `~/.converyn/config.json`; with sudo, the invoking user's home and ownership are used. The first context becomes current, while an existing selection is preserved. Exact repeated imports are idempotent; a same-name context with different credentials is retained and reported instead of being overwritten. Use `--no-default-context` to skip this local handoff.

```bash
converyn status
converyn get nodes
converyn get workflows
converyn get runs
converyn config get-contexts
```

Use `converyn config use-context YOUR_CONTEXT` to select another imported context. `converyn agent --config PATH` starts the host-local daemon; it is not a node-listing command. Remote-only deployment does not install a CLI on the operator if that host is not a service target; install the same binary or run the snapshot from the installation record.

For an earlier completed installation, import its existing OP once as the account that will run CLI commands (replace paths and the context name):

```bash
converyn config import /path/to/installation/operator/config.json --name YOUR_CONTEXT
converyn config get-contexts
```

The source must be readable by that account. Root-owned installation records remain private; do not make the record directory world-readable. The explicit original path also remains usable with `converyn --config /path/to/installation/operator/config.json status`.

The created OP is imported, not reissued. Additional people, custom context stores, Nodes and browser materials are handled by the [initializer](init-script.md); see the [complete walkthrough](../tutorials/quickstart.md) for host-group/workflow registration and execution. `--prepare-only` has not started controllers, initialized membership or created OP; complete that same plan with `--resume` first.

## Recovery and installation records

```bash
sudo bash /secure/converyn-install/qa/install.sh deploy \
  --binary /secure/converyn-install/qa/converyn \
  --cluster qa --operator-name OP --installation /secure/converyn-install/qa \
  --member 1=127.0.0.1:8443 --controller 1=local --agent node-a=local \
  --resume --yes --non-interactive
```
The directory retains frozen `install.sh`, `converyn`, `plan.tsv`, original CA/keyring, personal OP configuration, bootstraps and checkpoints. Recovery reuses these identities and refuses mixed installer/binary versions; invoke the retained copies. Completed Agent enrollment is verified through fresh heartbeats rather than repeated supervisor installation. Lost SSH staging can be recreated safely from original inputs.

Directories use 0700 and credentials/diagnostics 0600. Do not commit or distribute the full installation record to ordinary nodes. Bootstraps last one hour. Expiration, partial signing material, changed host configuration or unknown execution results require inspection; the script stops and retains evidence instead of deleting state, regenerating CAs or recovering another Node automatically. Use [operations](../how-to/maintenance.md) for upgrades and [getting started](../how-to/installation.md) for manual installation.

## Validation

```bash
bash -n scripts/ops/install.sh
python3 -m unittest discover -s scripts/tests/install -p 'test_*.py' -v
env CONVERYN_INSTALL_LIVE=1 python3 -m unittest discover \
  -s scripts/tests/install -p 'test_*.py' -v
```
Maintainer fixtures use real temporary single/three-member controllers, personal mTLS identities and Agents, replacing systemd/SSH host installation to avoid host mutation. They verify bundles, initialization, per-entry reads, heartbeats and resume without claiming production service installation. See [regressions](../development/testing.md).
