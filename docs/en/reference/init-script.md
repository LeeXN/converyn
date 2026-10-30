# Personal identity and Node initialization

[English](init-script.md) | [简体中文](../../zh-CN/reference/init-script.md)

[init.sh](../../../scripts/ops/init.sh) invokes the existing CLI on the operator machine for personal issuance, server-side grants, local context import, Node delivery and browser credential export. Deploy the platform with [install.sh](../../../scripts/ops/install.sh); see [installation to first deployment](../tutorials/quickstart.md) for end-to-end commands. Initialization requires a running, reachable controller.

## Shared Shell interface

The two Shell entry points share `--binary`, `--installation`, `--yes`, `--non-interactive` and `--help`. Installation records use `--installation` for creation and later reading; `init.sh` use `--output` for a new delivery directory. SSH selectors are `--ssh-host`, `--ssh-user`, `--ssh-key`, `--ssh-port` (22) and `--ssh-known-hosts` (`~/.ssh/known_hosts`); deploy uses topology selectors instead of a single `--ssh-host`. Underlying `converyn admin` CLI flags retain their native names.

No arguments with a terminal opens the wizard. Explicit actions can prompt for missing required inputs. `--yes` accepts the plan; `--non-interactive` forbids prompts, so unattended execution uses both. Options for another action are rejected. Binary discovery checks an explicit path, PATH, the adjacent release and then repository build outputs; an invalid explicit binary is never replaced by a fallback.

Progress, diagnostics and underlying command output go to stderr. After success, stdout contains one text summary with `Action`, `Result`, relevant record/output/config paths and the next command. Failure leaves stdout empty. Exit codes are 0 for success/help, 2 for invalid/missing inputs, and 1 for execution failure or cancellation. Cancellation precedes persistent installation records, issuance and host installation. Existing private records retain their format and checksum checks; recovery still requires the original frozen installer and binary.


## Prerequisites and wizard

Requires Linux, Bash 4.4+, GNU coreutils, grep, sed, flock and an executable Converyn binary. Browser P12 export also requires OpenSSL. Python/jq are unnecessary. Node delivery requires `init.sh` and `install.sh` in the same directory.

```bash
bash scripts/ops/init.sh --help
bash scripts/ops/init.sh --binary ./converyn-linux-amd64 \
  --installation "$HOME/.local/share/converyn-install/qa"
```

Without an action, the wizard opens. `--binary` selects a binary explicitly; otherwise discovery checks PATH for `converyn`, then the matching release binary beside the script, `converyn` beside the script, repository `target/release/converyn`, and `target/debug/converyn`. Specify `--binary` when selecting the build for the current installation matters.

Use `sudo` for root-owned installation records and specify `--contexts` to avoid accidentally importing into root's default configuration. Private input files must be 0600; output parent directories must belong to the invoking user with mode 0700. Use normalized absolute paths without spaces or symlinks and ancestors that other users cannot write; root-owned sticky directories such as `/tmp` are permitted.

## Choose an action

| Action | Inputs and behavior | Output |
| --- | --- | --- |
| `import` | `--file` or the installation's existing OP; verifies a named personal identity before import | Local context under the selected alias; no issuance or grants |
| `operator` | Manually initialized bootstrap configuration and `--authority`; first named OP only | Super administrator credentials and context |
| `user` | Named administrator configuration, issuer, name, role and scope | Separate personal credentials, role binding and context |
| `node` | Platform-authorized configuration, new `--node` and `--output` | Node delivery directory; no remote service installation or host-group membership |
| `browser` | Recipient's `--file` and new `--output` directory | `README.md`, `ca.crt`, `client.p12`, `token.txt` |

The installer normally imports its OP into the operator default CLI config. Use `import` to place that same OP into another context alias or config file. With an existing named OP, create another administrator using `user --role super_admin`. `operator` does not rebuild or recover an existing administrator; see [certificates](../how-to/certificates.md) for rotation and administrator recovery.

## Configuration, scopes and files

| Option | Meaning |
| --- | --- |
| `--installation DIR` | Installation record root; defaults to `operator/config.json` and `bundle/user-authority`, selecting bootstrap configuration only if OP configuration is absent |
| `--admin-config FILE`, `--admin-context NAME` | Explicit management identity and context, overriding the installation's management configuration selection |
| `--authority DIR` | Override personal issuer path; keep it on the issuing operator machine |
| `--contexts FILE` | Import destination; defaults to `CONVERYN_CONFIG`, then `~/.converyn/config.json` |
| `--file FILE`, `--source-context NAME` | Source personal configuration and context for import or browser export |
| `--context-name NAME`, `--use` | Destination alias and whether to select it; aliases do not change server permissions |
| `--name NAME`, `--output DIR` | New identity display name and credential directory; do not reuse existing outputs |
| `--role ROLE`, `--workflow ID`, `--preset NAME` | Business permission scope; presets may repeat; see [access](../how-to/access.md) for roles |
| `--all-workflows` | Explicit business grant across all workflows, mutually exclusive with `--workflow` |
| `--valid-for DURATION` | Personal certificate lifetime, default `365d`, still subject to CLI and issuer limits |
| `--hardware-id-file FILE` | Path to an existing unique identity file on the destination Agent host |
| `--ttl-seconds SECONDS` | One-use Node bootstrap lifetime, 60–3600 seconds, default 3600 |
| `--password-file FILE` | P12 password file, 0600; required for noninteractive browser export |

`viewer`, `executor` and `maintainer` require explicit `--workflow` or `--all-workflows`; only the first two allow `--preset` restrictions. `platform_operator`, `access_admin` and `super_admin` are instance-scoped and reject workflow/preset arguments. `user` submits a role binding after issuance; direct `create identities` grants no business permissions by default.

Deliver only the personal output's `config.json` to its recipient, rather than the operator's multi-person contexts file. Node delivery contains `converyn`, `install.sh`, `bootstrap.json`, `INSTALL.txt`, `checksums.sha256`. Follow `INSTALL.txt` to verify/install on the destination and check authenticated heartbeats; update platform node groups separately.

## Recovery and troubleshooting

`operator`, `user` and `node` save `OUTPUT.init-plan.tsv` and a lock file beside the output directory. After interruption retain output and records, rerun with the same script, binary, identity and arguments, and add `--resume`; changed inputs are rejected. Inspect partial material without a valid operation record rather than deleting and reissuing it.

| Situation | Action |
| --- | --- |
| `Expected ... 0700/0600` | Check invoking user, ancestor directories and permissions; keep credential access private |
| Context alias already exists | Inspect the alias first; use `--replace-context` for an intentional replacement, preserving other aliases |
| Import fails or controller is unreachable | Check selected binary, endpoints, CA, named certificate and matching token; bootstrap configuration cannot become a personal identity |
| `Business roles need ...` | Specify workflow or global scope and select preset restrictions supported by the role |
| Bootstrap expired/consumed or Node already exists | Inspect the original bootstrap and Node state; `--resume` neither extends expiry nor automatically recovers an old Node identity |
| Browser output already exists | `browser` does not resume/overwrite exports; retain and inspect the original package and choose a new directory for another export |

Automation requires an explicit action, complete arguments and `--yes --non-interactive`; these do not bypass file or server authorization checks. `--resume` recovers the original operation; rotate credentials through CLI `rotate credentials`. The full script option list is `bash scripts/ops/init.sh --help`; underlying commands are in the [CLI reference](cli.md).

## Maintainer validation

```bash
bash -n scripts/ops/init.sh
python3 -m unittest discover -s scripts/tests/init -p 'test_*.py' -v
env CONVERYN_INSTALL_LIVE=1 python3 -m unittest discover \
  -s scripts/tests/init -p 'test_*.py' -v
```

Real mTLS fixtures require a built `target/debug/converyn`, loopback listeners and subprocess support. They reuse the [installer fixture](../development/testing.md) to check first OP, separate personal certificates, permission scopes, context preservation, Node delivery and browser exports, without modifying host systemd or connecting to existing private environments.

## Remote Node enrollment

SSH enrollment checks connectivity before issuing the original Node delivery and confirms a fresh authenticated heartbeat after installation. Output defaults to `nodes/UID` beneath the installation record; an explicit new `--output` is also supported. Password/sudo authentication requires a terminal. Unattended use requires verified host keys and key authentication. Preserve the original delivery on failure and use the original arguments with `--resume`; do not replace an existing Node UID.

```bash
bash scripts/ops/init.sh node --binary /path/to/converyn \
  --installation /secure/converyn-install/qa --node node-b \
  --ssh-host 192.0.2.11 --ssh-user operator --ssh-auth key \
  --ssh-known-hosts /secure/known_hosts --yes --non-interactive
```
