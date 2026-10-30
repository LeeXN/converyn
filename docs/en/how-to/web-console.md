# Web console

[English](web-console.md) | [简体中文](../../zh-CN/how-to/web-console.md)

## Login

GUI shares the controller HTTPS listener. OP issues a [personal certificate/token pair](certificates.md). Import its PKCS#12 package into the browser computer's personal certificate store and verify/trust the CA. Windows uses the current user's personal store in certmgr.msc.

Select the personal certificate, enter the control-plane name and matching token. Tokens are kept in tab-scoped session storage so refresh preserves the current route and login. The server revalidates identity before loading business pages; expiry, revocation or sign-out clears the session. Tokens are never written to local storage. Closing the tab ends its session; a browser may restore tab sessions when restoring closed tabs. Bootstrap credentials cannot be used for routine GUI login; first create a named administrator through CLI. The console shows the person name, role and credential expiry. The language selector switches between Simplified Chinese and English and remembers the preference.

The package password is for import, not API login. Check the private key, issuer, validity and browser selection for TLS client-certificate errors. For Unauthorized, check cluster name, pairing and disabled state.

ERR_SSL_DECRYPT_ERROR_ALERT before the page loads can occur when a browser presents a retired client certificate after an environment rebuild. Check the current access inventory, import that environment's current P12 and trust its CA. Close all browser processes before selecting the current certificate again. On Windows, remove the corresponding retired personal certificate in certmgr.msc and clear cached selection using Internet Options → Content → Clear SSL state. Once the page opens, use the current token for the same identity.

OP can generate login materials with `init.sh browser --file <recipient-config> --output <new-directory>` or `auth gui-login --with-credentials`. Prepare a private output parent first; enter the P12 password interactively or use a 0600 password file for automation. See the [initializer](../reference/init-script.md) for inputs and recovery rules and the [walkthrough](../tutorials/quickstart.md) for delivery steps.

## Execute an existing workflow

1. Choose an enabled registered workflow.
2. Select one of its target presets.
3. Edit variables in the Run configuration K/V form, restore defaults or add execution-only variables. Immutable values are read-only. Existing configuration files expose allowed fields as inputs. Fixed configuration and provenance remain read-only.
4. Preview the actual target, final variables and work items, including script invocation views with each node's effective environment.
5. Execute and follow the original Run, person attribution, receipts and logs.

There is no approval step. Preview checks configuration consistency; changing inputs or source/target revisions invalidates it. Overrides never become stored defaults.

## Pages and control

Workflows expose current revisions and execution forms. Runs show all tasks, submitters, snapshots, work and receipts. Nodes show Agent and host inventory. External targets show registered external systems such as Kubernetes APIs; these are distinct from workflow target presets. Audit traces authenticated operations; control-plane membership and replication progress are available to administrators in tables. These do not establish individual member health.

Run lists and details show total elapsed time from submission to completion, including queueing and pauses. Active tasks update every second; historical tasks without the required timestamps show a dash. Select a step to highlight its row and focus the separate details panel. Operation fields and native before/after evidence explain what ran; technical metadata and the raw receipt are collapsed. Copying a work ID reports success or failure. Script steps show stdout/stderr, complete saved-log downloads, and explicit empty, waiting, failure or receipt-tail states.

Executors pause/resume/cancel only their own Runs; administrators control all and resolve unknown outcomes using original evidence. Cancellation does not roll back completed effects. Pausing/canceling an opted-in workflow also stops further repair for that generation.

Workflows, runs, nodes, external targets and audit lists share page-size choices of 10/20/50/100, page numbers, item ranges and first/previous/next controls. Long lists have controls above and below the data. Loading prevents duplicate navigation; failures allow returning to an earlier page. Detail navigation, list switching and reload retain each person's page and size for that control plane.

The run list shows execution and end dates, paginated by submission time with the newest first. The end date is recorded when a run succeeds, fails or finishes canceling. Unfinished runs and legacy runs without a recorded date show “—”; archive time is never substituted.

Polling displays freshness, failures and the next retry. Stale data cannot submit mutations. Navigation cancels old requests. Lists paginate; logs support live streaming and saved-content download. Configuration, audit entries and receipts use labeled fields rather than JSON dumps. Identifiers, user configuration values and original log output retain their original content.

The console does not edit workflows, scripts, persistent configuration or personal credentials. OP maintains these through source files/Git and CLI. See [resources](../reference/resources.md) and [CLI](../reference/cli.md).

Node lists show heartbeat, addresses, CPU, memory, GPU and OS. Open a node or visit `#/nodes/NODE` for grouped assets, serials, device lists, collection errors and freshness. Changing nodes cancels the old read and keeps its response out of the new page. Run rows follow the plan's step/node/phase/operation order, with status, exit and reason; copy retains the full Work ID. Log viewing reconnects at accepted byte offsets without duplicating output, labels live fragments and receipt tails, and distinguishes expiration, missing uploads and truncation. Full downloads verify the returned SHA256 and do not silently substitute receipt tails.

## External targets and workflow presets

An external target identifies an existing platform and references its connection configuration. A typical use is deploying an application to an existing Kubernetes cluster: the controller calls its API through a plugin to create or update Deployments and check rollout status.

| Operation | Execution destination | How it runs |
| --- | --- | --- |
| Write configuration, manage services or run scripts on VMs or physical hosts | Host cluster and nodes selected by the workflow preset | Converyn Agents on the hosts |
| Deploy applications to an existing Kubernetes cluster | External target selected by the workflow preset | Controller calls the Kubernetes API |

A target preset is a workflow option, such as test or production, that selects the hosts or platform for a run. A host Cluster groups machines for Agent execution. An ExternalTarget records platform identity and a connection reference. A mixed workflow preset can bind both; the execution preview shows the actual location of each step.

For Kubernetes deployment, an operator prepares the connection configuration, registers the external target and its identity, and binds a workflow preset to it. Executors select an existing preset, preview the plan and run it. Example: `production preset → production Kubernetes target → that cluster's API`. Operators maintain target registrations and access credentials; see [plugins and connections](plugins.md).

The three lab demo workflows use the ha, single and aio presets to select their respective hosts Clusters. Agents perform the file and script operations, so those host workflows can run with an empty external-target registry. The external-target page explains both scenarios; workflow details show the selected binding.

## Generate recipient instructions with CLI

Run `converyn auth gui-login --output-dir ./gui-login` with the intended person's client configuration to generate instructions and the public CA matching the actual environment, identity and certificate. Use `--language en` for English and explicitly add `--with-credentials` to include that person's P12 and bearer file; deliver the password separately. The default guide contains no login secrets. Deliver private packages only to their intended owners. Export just the CA with `converyn export credentials --ca --output ./ca.crt`. See [personal certificates](certificates.md).

Workflow defaults, node defaults and locked paths use `values`, `node_values` and `immutable`. Untouched fields retain node defaults; explicit edits override them. Restore defaults removes execution overrides. Logs record effective variables with Secret values redacted. See [workflow values](../../../example/workflow/README.md).

The parameter area shows a list only when variables exist. The add form supports typed values, Enter to add, and focused field errors. One Restore defaults action clears edits to variables, step parameters and file content. The form uses a single column on phones.

## Management and grants

Navigation follows effective capabilities. Access management handles people, groups and role bindings; host group management handles membership and node retirement; external targets can register Kubernetes identities using existing controller connections. Workflow maintainers can enable/disable workflows. Author workflow files and scripts in files/Git, register through CLI and issue personal credentials through CLI. See [People and workflow access](access.md). List filters run on the server and can be shared through page URLs. Run details prioritize failed or attention-required work and support step deep links.

Host storage mounts include common persistent local filesystems and host NFS/CIFS shares. Collection excludes tmpfs, container overlays, Pod volumes and runtime-internal mounts. Bind aliases of one block filesystem are deduplicated; separate partitions, Btrfs subvolumes, ZFS datasets and distinct share sources remain. Inventory and resource samples share this selection before filesystem usage calls. Collection does not mount or modify storage.

Status shows collection age, such as seconds since an update or minutes since an inventory check. Omitted details identify the affected field and are separate from collection failures. Details show reported entries without presenting an omitted list as an absent device. Existing observation limits and collection intervals remain unchanged.
