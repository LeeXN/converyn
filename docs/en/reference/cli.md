# CLI reference

[English](../../en/reference/cli.md) | [简体中文](../../zh-CN/reference/cli.md)

Options are generated from the actual binary. Clients select `--config`, then `CONVERYN_CONFIG`, then `~/.converyn/config.json`; `--context` temporarily overrides the saved selection. Personal configurations embed certificates, keys and tokens and require mode 0600. Clients accept portable context profiles. Server/agent/admin renew use explicit, separate service configurations. See [getting started](../how-to/installation.md).

See [usage](../how-to/runs.md) for common commands, values, retries, exit codes and logs, and [authoring](../how-to/workflows.md) for workflow/script contracts. This reference owns complete commands and options. Output defaults to tables; programs should select `-o json`. `--output` still selects a file path. Resource operations use verb + resource; maintenance operations use `admin`.

## `get`

List resources or inspect one by ID

```text
Usage: converyn get [OPTIONS] <COMMAND>

Commands:
  secrets      List Secret names and versions without exposing values
  artifacts    List immutable artifacts uploaded to Converyn
  workflows    List workflow names, targets and revisions; accepts an ID for details
  runs         List Runs, newest submission first (unknown submission times last); accepts an ID for details
  nodes        List node heartbeat, IP, CPU, memory, GPU and OS; accepts an ID for grouped asset details
  node-groups  List node groups and revisions; accepts an ID for details
  identities   List people; use an identity UID or unique name for details and their certificates
  credentials  List registered certificates; use a credential UID for metadata, storage and export instructions
  members      List control-plane members and voters; accepts a member ID for details
  targets      List external targets; accepts an ID for details
  plugins      List installed plugins and fixed contracts; accepts a plugin ID for full details
  history      Read retained history and retention settings
  audit        Read immutable audit pages
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get secrets`

```text
Usage: converyn get secrets [OPTIONS] [NAME]

Arguments:
  [NAME]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get artifacts`

```text
Usage: converyn get artifacts [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get workflows`

```text
Usage: converyn get workflows [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --limit <LIMIT>                  Return one page (1..100); default reads all pages
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --cursor <CURSOR>                Continue one page; preserve filters
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --search <SEARCH>
      --status <STATUS>                [possible values: enabled, disabled, all]
  -h, --help                           Print help
```

### `get runs`

```text
Usage: converyn get runs [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --workflow <WORKFLOW>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --preset <PRESET>
      --mine
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --limit <LIMIT>                  Return one page (1..100); default reads all pages
      --cursor <CURSOR>                Continue one page; preserve filters
      --status <STATUS>                Filter the committed Run state [possible values: pending, preparing, paused, authorized, running, cancel_requested, needs_attention, succeeded, failed, canceled]
  -h, --help                           Print help

Run detail columns: STEP / NODE / PHASE / OPERATION / STATUS / EXIT / REASON. Use -o wide to include full Work IDs.
Valid states: pending, preparing, paused, authorized, running, cancel_requested, needs_attention, succeeded, failed, canceled.
Logs: converyn logs runs RUN --step verify --follow
Pagination: default reads all pages; --limit returns one page; reuse --cursor with the same --status.
```

### `get nodes`

```text
Usage: converyn get nodes [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --limit <LIMIT>                  Return one page (1..100); default reads all pages
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --cursor <CURSOR>                Continue one page; preserve filters
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --status <STATUS>                Filter heartbeat observation [possible values: online, offline]
  -h, --help                           Print help

Examples:
  converyn get nodes
  converyn get nodes NODE
  converyn get nodes -o wide
  converyn get nodes -o json
Heartbeat: Online <60s, Offline >=60s, Unknown without an observation.
Inventory is checked every 5 minutes (stale after 10 minutes); resources are sampled every 20 seconds (stale after 60 seconds). Missing fields and collection errors are reported separately.
Pagination: default reads all pages; --limit returns one page; reuse --cursor with the same --status.
```

### `get node-groups`

```text
Usage: converyn get node-groups [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --limit <LIMIT>                  Return one page (1..100); default reads all pages
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --cursor <CURSOR>                Continue one page; preserve filters
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get identities`

```text
Usage: converyn get identities [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get credentials`

```text
Usage: converyn get credentials [OPTIONS] [ID]

Arguments:
  [ID]  Credential UID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --identity <IDENTITY>            Filter by identity UID or unique name
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help

Examples:
  converyn get credentials
  converyn get credentials --identity OP
  converyn get credentials CREDENTIAL_ID
  converyn export credentials --id CREDENTIAL_ID --output ./public.crt
  converyn export credentials --current --output ./browser.p12
The controller records public certificates and metadata, not client private keys or original client filesystem paths. Detail checks whether the selected local config holds a matching private credential.
```

### `get members`

```text
Usage: converyn get members [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get targets`

```text
Usage: converyn get targets [OPTIONS] [ID]

Arguments:
  [ID]  Resource ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --limit <LIMIT>                  Return one page (1..100); default reads all pages
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --cursor <CURSOR>                Continue one page; preserve filters
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get plugins`

```text
Usage: converyn get plugins [OPTIONS] [ID]

Arguments:
  [ID]  Plugin ID; omit to list

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --local                          Inspect plugins compiled into this binary without contacting a controller
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get history`

```text
Usage: converyn get history [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `get audit`

```text
Usage: converyn get audit [OPTIONS]

Options:
      --after <AFTER>                  [default: 0]
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --limit <LIMIT>                  [default: 100]
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `status`

Show the current cluster overview

```text
Usage: converyn status [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `validate`

Validate local workflow sources

```text
Usage: converyn validate [OPTIONS] <COMMAND>

Commands:
  workflows  Validate workflows
  help       Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `validate workflows`

```text
Usage: converyn validate workflows [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>                    Workflow source file or directory containing workflow.yaml
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `register`

Register a workflow snapshot

```text
Usage: converyn register [OPTIONS] <COMMAND>

Commands:
  secrets    Encrypt and register an immutable Secret version once for the cluster
  artifacts  Upload one local artifact to Converyn; print its immutable digest and size
  workflows  Register workflows
  help       Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `register secrets`

```text
Usage: converyn register secrets [OPTIONS] --version <VERSION> --file <FILE> <NAME>

Arguments:
  <NAME>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --version <VERSION>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -f, --file <FILE>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `register artifacts`

```text
Usage: converyn register artifacts [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `register workflows`

```text
Usage: converyn register workflows [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>                    Workflow source file or directory containing workflow.yaml
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `create`

Create identities or credentials

```text
Usage: converyn create [OPTIONS] <COMMAND>

Commands:
  identities   Create identities
  credentials  Create credentials
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `create identities`

```text
Usage: converyn create identities [OPTIONS] --authority <AUTHORITY> --output <OUTPUT> --name <NAME>

Options:
      --authority <AUTHORITY>          [env: CONVERYN_AUTHORITY=]
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --output <OUTPUT>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --valid-for <VALID_FOR>
      --expires-at <EXPIRES_AT>
      --name <NAME>
      --super-admin                    Create a super administrator; ordinary identities start without grants
  -h, --help                           Print help
```

### `create credentials`

```text
Usage: converyn create credentials [OPTIONS] --authority <AUTHORITY> --output <OUTPUT> --identity <IDENTITY>

Options:
      --authority <AUTHORITY>          [env: CONVERYN_AUTHORITY=]
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --output <OUTPUT>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --valid-for <VALID_FOR>
      --expires-at <EXPIRES_AT>
      --identity <IDENTITY>
  -h, --help                           Print help
```

## `update`

Submit identity, node-group, target or history changes

```text
Usage: converyn update [OPTIONS] <COMMAND>

Commands:
  identities   Update identities
  node-groups  Submit a typed node-group management change, including its action and revision
  targets      Submit a typed external-target management change
  history      Submit a typed retention/archive/cleanup request with its revision and evidence
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `update identities`

```text
Usage: converyn update identities [OPTIONS] --name <NAME> <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --name <NAME>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `update node-groups`

```text
Usage: converyn update node-groups [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `update targets`

```text
Usage: converyn update targets [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `update history`

```text
Usage: converyn update history [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `delete`

Retire an unreferenced node group

```text
Usage: converyn delete [OPTIONS] <COMMAND>

Commands:
  node-groups  Retire an unreferenced node group, retaining its UID and audit history
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `delete node-groups`

```text
Usage: converyn delete node-groups [OPTIONS] --expected-revision <EXPECTED_REVISION> --reason <REASON> <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --expected-revision <EXPECTED_REVISION>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --reason <REASON>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help
          Print help
```

## `enable`

Enable workflows or identities

```text
Usage: converyn enable [OPTIONS] <COMMAND>

Commands:
  workflows   Enable workflows
  identities  Enable identities
  help        Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `enable workflows`

```text
Usage: converyn enable workflows [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `enable identities`

```text
Usage: converyn enable identities [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `disable`

Disable workflows or identities

```text
Usage: converyn disable [OPTIONS] <COMMAND>

Commands:
  workflows   Disable workflows
  identities  Disable identities
  help        Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `disable workflows`

```text
Usage: converyn disable workflows [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `disable identities`

```text
Usage: converyn disable identities [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `rotate`

Rotate personal credentials

```text
Usage: converyn rotate [OPTIONS] <COMMAND>

Commands:
  credentials  Rotate credentials
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `rotate credentials`

```text
Usage: converyn rotate credentials [OPTIONS] --authority <AUTHORITY> --output <OUTPUT> --identity <IDENTITY>

Options:
      --authority <AUTHORITY>          [env: CONVERYN_AUTHORITY=]
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --output <OUTPUT>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --valid-for <VALID_FOR>
      --expires-at <EXPIRES_AT>
      --identity <IDENTITY>
      --overlap <OVERLAP>              [default: 0s]
  -h, --help                           Print help
```

## `revoke`

Revoke a personal credential

```text
Usage: converyn revoke [OPTIONS] <COMMAND>

Commands:
  credentials  Revoke credentials
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `revoke credentials`

```text
Usage: converyn revoke credentials [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `export`

Export certificates, CA or private credentials

```text
Usage: converyn export [OPTIONS] <COMMAND>

Commands:
  credentials  Export a public certificate, CA, or private PKCS#12 bundle
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `export credentials`

```text
Usage: converyn export credentials [OPTIONS] --output <OUTPUT> <--id <ID>|--current|--ca|--directory <DIRECTORY>>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --id <ID>                        Export this registered public certificate as PEM; no private key or token
      --ca                             Export the selected context's public CA certificate bundle as PEM; no private key or token
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --current                        Export a PKCS#12 bundle from the selected local --config/--context, including its private key
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --directory <DIRECTORY>          Export PKCS#12 from the original private bundle's config.json; independent of client contexts
      --output <OUTPUT>                New output file; existing files and symlinks are never overwritten
      --password-file <PASSWORD_FILE>  Read the PKCS#12 password from a private file; omit for an interactive OpenSSL prompt
  -h, --help                           Print help

Examples:
  converyn export credentials --id CREDENTIAL_ID --output ./public.crt
  converyn export credentials --ca --output ./ca.crt
  converyn export credentials --current --output ./browser.p12
  converyn --context qa export credentials --current --output ./qa.p12
  converyn export credentials --directory /secure/alice --output ./alice.p12
The server cannot recover private keys or tokens. --current reads your selected local config; --directory reads the original private bundle. PKCS#12 export uses installed OpenSSL and prompts for a password unless --password-file is supplied.
```

## `replace`

Submit an audited controller member replacement

```text
Usage: converyn replace [OPTIONS] <COMMAND>

Commands:
  members  Submit an audited controller replacement with membership and certificate checks
  help     Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `replace members`

```text
Usage: converyn replace members [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `retire`

Retire an unused Node identity

```text
Usage: converyn retire [OPTIONS] <COMMAND>

Commands:
  nodes  Retire nodes
  help   Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `retire nodes`

```text
Usage: converyn retire nodes [OPTIONS] --expected-fingerprint <EXPECTED_FINGERPRINT> --evidence <EVIDENCE> <NODE>

Arguments:
  <NODE>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --expected-fingerprint <EXPECTED_FINGERPRINT>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --evidence <EVIDENCE>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help
          Print help
```

## `run`

Submit workflows; --wait waits, --dry-run previews

```text
Usage: converyn run [OPTIONS] <COMMAND>

Commands:
  workflows  Submit a workflow; --wait waits for its result and --dry-run only previews
  help       Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `run workflows`

```text
Usage: converyn run workflows [OPTIONS] <WORKFLOW>

Arguments:
  <WORKFLOW>  Workflow ID

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --target <TARGET>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
  -f, --values <FILE>
          Merge YAML/JSON values files from left to right; later files win
  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --set <SETS>
          Override or add values (KEY=VALUE, nested keys use dots); overrides files
      --set-string <SET_STRINGS>
          Like --set, preserving string values such as numeric version identifiers
      --request-id <REQUEST_ID>

      --request-dir <REQUEST_DIR>
          [env: CONVERYN_REQUEST_DIR=]
      --dry-run

      --wait

      --timeout-seconds <TIMEOUT_SECONDS>

  -h, --help
          Print help
```

## `watch`

Wait for Run evidence and return its outcome

```text
Usage: converyn watch [OPTIONS] <COMMAND>

Commands:
  runs  Wait for actual Run evidence. Exit 0=success, 2=failed/canceled, 3=attention, 124=timeout
  help  Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `watch runs`

```text
Usage: converyn watch runs [OPTIONS] <RUN_ID>

Arguments:
  <RUN_ID>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --timeout-seconds <TIMEOUT_SECONDS>
          [default: 1800]
      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help
          Print help
```

## `logs`

Read, follow or export Run logs

```text
Usage: converyn logs [OPTIONS] <COMMAND>

Commands:
  runs  Read saved logs by step/node or exact Work; defaults to the last 100 lines per stream
  help  Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `logs runs`

```text
Usage: converyn logs runs [OPTIONS] <RUN_ID>

Arguments:
  <RUN_ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --work-id <WORK_ID>              Exact Work ID; mutually exclusive with --step and --node
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --step <STEP>                    Select a named plan step, optionally restricted by --node
      --node <NODE>                    Select a node, optionally restricted by --step
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --tail <TAIL>                    Recent lines per Work and stream (default: 100)
      --stream <STREAM>                Select output streams [default: both] [possible values: stdout, stderr, both]
      --full                           Read all saved bytes, verifying the final manifest. Retention and truncation still apply
      --follow                         Follow using acknowledged byte cursors; JSON is NDJSON. Ctrl+C stops viewing only
      --output-dir <OUTPUT_DIR>        Export complete saved streams into separate Work/stream files; refuses overwrites
  -h, --help                           Print help

Examples:
  converyn logs runs RUN
  converyn logs runs RUN --step verify --node single-agent-1
  converyn logs runs RUN --step verify --follow
  converyn logs runs RUN --tail 200 --stream stderr
  converyn logs runs RUN --full
  converyn logs runs RUN --full --output-dir ./run-logs
  converyn logs runs RUN --follow -o json
Full means all saved bytes, subject to the existing 8 MiB stream cap and retention. Full reads/export never fall back to receipt tails. Default tail reads label any receipt fallback. Follow reconnects at byte cursors; Ctrl+C only stops viewing.
```

## `cancel`

Request Run cancellation

```text
Usage: converyn cancel [OPTIONS] <COMMAND>

Commands:
  runs  Cancel runs
  help  Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `cancel runs`

```text
Usage: converyn cancel runs [OPTIONS] <RUN_ID>

Arguments:
  <RUN_ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `pause`

Pause new Run authorizations

```text
Usage: converyn pause [OPTIONS] <COMMAND>

Commands:
  runs  Pause runs
  help  Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `pause runs`

```text
Usage: converyn pause runs [OPTIONS] <RUN_ID>

Arguments:
  <RUN_ID>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --expected-revision <EXPECTED_REVISION>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help
          Print help
```

## `resume`

Resume Run authorizations or a credential operation

```text
Usage: converyn resume [OPTIONS] <COMMAND>

Commands:
  credentials  Resume credentials
  runs         Resume paused Runs or re-admit never-started work after a pure input failure
  help         Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `resume credentials`

```text
Usage: converyn resume credentials [OPTIONS] --output <OUTPUT>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --output <OUTPUT>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `resume runs`

```text
Usage: converyn resume runs [OPTIONS] <RUN_ID>

Arguments:
  <RUN_ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `resolve`

Resolve uncertain Work with evidence

```text
Usage: converyn resolve [OPTIONS] <COMMAND>

Commands:
  runs  Resolve runs
  help  Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `resolve runs`

```text
Usage: converyn resolve runs [OPTIONS] --work-id <WORK_ID> --status <STATUS> --evidence <EVIDENCE> <RUN_ID>

Arguments:
  <RUN_ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --work-id <WORK_ID>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --status <STATUS>                [possible values: succeeded, failed, canceled]
      --evidence <EVIDENCE>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `compile`

Compile PlanInput or fixed plugin calls

```text
Usage: converyn compile [OPTIONS] <COMMAND>

Commands:
  plans    Compile local PlanInput YAML/JSON into an immutable Plan; does not execute it
  plugins  Compile a plugin spec into a fixed call descriptor; does not execute it
  help     Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `compile plans`

```text
Usage: converyn compile plans [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `compile plugins`

```text
Usage: converyn compile plugins [OPTIONS] --action <ACTION> --spec <SPEC> <ID>

Arguments:
  <ID>

Options:
      --action <ACTION>
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --spec <SPEC>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `prepare`

Generate private plugin inputs

```text
Usage: converyn prepare [OPTIONS] <COMMAND>

Commands:
  plugins  Generate local private plugin inputs; does not contact the control plane
  help     Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `prepare plugins`

```text
Usage: converyn prepare plugins [OPTIONS] --action <ACTION> --input <INPUT> --output <OUTPUT> <ID>

Arguments:
  <ID>

Options:
      --action <ACTION>
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --input <INPUT>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --output <OUTPUT>
  -h, --help                           Print help
```

## `expand`

Expand plugins into local Playbooks

```text
Usage: converyn expand [OPTIONS] <COMMAND>

Commands:
  plugins  Generate a local Playbook and embedded definitions; does not execute them
  help     Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `expand plugins`

```text
Usage: converyn expand plugins [OPTIONS] --action <ACTION> --input <INPUT> --output <OUTPUT> <ID>

Arguments:
  <ID>

Options:
      --action <ACTION>
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --input <INPUT>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --output <OUTPUT>
  -h, --help                           Print help
```

## `config`

Manage personal contexts

```text
Usage: converyn config [OPTIONS] <COMMAND>

Commands:
  import           Import personal credentials without changing an existing current context
  get-contexts     List context names, endpoints and the selected context; no credentials
  current-context  Print the saved current context
  use-context      Persist the current context; --context only selects one command temporarily
  view             Print configuration with private keys and tokens redacted
  export           Export only the selected context to a new private, self-contained JSON file
  help             Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `config import`

```text
Usage: converyn config import [OPTIONS] --name <NAME> <FILE>

Arguments:
  <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --name <NAME>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --use
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --replace
  -h, --help                           Print help
```

### `config get-contexts`

```text
Usage: converyn config get-contexts [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `config current-context`

```text
Usage: converyn config current-context [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `config use-context`

```text
Usage: converyn config use-context [OPTIONS] <NAME>

Arguments:
  <NAME>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `config view`

```text
Usage: converyn config view [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `config export`

```text
Usage: converyn config export [OPTIONS] --output <OUTPUT>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --output <OUTPUT>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `auth`

Inspect permissions, manage access or export GUI login instructions

```text
Usage: converyn auth [OPTIONS] <COMMAND>

Commands:
  gui-login  Export GUI login instructions and optional credentials for the selected personal identity
  status     Show this identity's effective bindings and their source
  can-i      Check a workflow or platform capability without performing it
  list       List groups, role bindings and the current access revision
  apply      Apply one typed access change from YAML/JSON, including expected_revision
  grant      Assign a predefined role to a person or group
  revoke     Remove a role binding
  group      Create or replace a user group and its complete membership
  doctor     Diagnose the selected context and report safe next commands
  help       Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth gui-login`

```text
Usage: converyn auth gui-login [OPTIONS] --output-dir <OUTPUT_DIR>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --output-dir <OUTPUT_DIR>        New delivery directory; publishes README.md and ca.crt
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --with-credentials               Also include the selected local user's private client.p12 and token.txt
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --password-file <PASSWORD_FILE>  Private password file for client.p12; omit for an interactive OpenSSL prompt
      --language <LANGUAGE>            Language of the generated login instructions [default: zh-CN] [possible values: zh-CN, en]
  -h, --help                           Print help

Examples:
  converyn auth gui-login --output-dir ./gui-login
  converyn --context alice auth gui-login --output-dir ./alice-gui --with-credentials
  converyn auth gui-login --output-dir ./gui-login --language en
The command confirms a named personal identity through mTLS. Default output contains README.md and ca.crt only. --with-credentials also exports the selected user's client.p12 and token.txt, and requires installed OpenSSL. Passwords and credential contents never appear in README.md or command output. Use the intended recipient's --config/--context; this command does not create or rotate identities. Existing output directories are never replaced.
```

### `auth status`

```text
Usage: converyn auth status [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth can-i`

```text
Usage: converyn auth can-i [OPTIONS] <ACTION>

Arguments:
  <ACTION>  [possible values: view, preview, execute, maintain, control, platform, access]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --workflow <WORKFLOW>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --preset <PRESET>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth list`

```text
Usage: converyn auth list [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth apply`

```text
Usage: converyn auth apply [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
  -f, --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth grant`

```text
Usage: converyn auth grant [OPTIONS] --role <ROLE> <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --person <PERSON>                Person UID or unique display name
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --group <GROUP>                  Group UID or unique display name
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --role <ROLE>                    [possible values: viewer, executor, maintainer, platform_operator, access_admin, super_admin]
      --workflow <WORKFLOW>
      --preset <PRESETS>
  -h, --help                           Print help
```

### `auth revoke`

```text
Usage: converyn auth revoke [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth group`

```text
Usage: converyn auth group [OPTIONS] --name <NAME> <ID>

Arguments:
  <ID>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --name <NAME>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --member <MEMBERS>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `auth doctor`

```text
Usage: converyn auth doctor [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `admin`

Install, initialize, back up and maintain Converyn

```text
Usage: converyn admin [OPTIONS] <COMMAND>

Commands:
  migrate-agent    Offline backed-up removal of obsolete Agent authorization settings
  output-key       Generate or rotate independent output encryption keys; distribute privately to all controllers
  init             Generate fresh controller bundles, bootstrap credentials and a personal issuer
  initialize
  bootstrap        Create a private one-use enrollment bundle; bearer values are written only to the output file
  enroll           Enroll a managed host; approved host steps may run as root. Retains journal and Node UID
  renew            Stage and activate a new Node certificate while retaining the Node identity
  install-local    Install this binary as a root supervisor, with a dedicated non-root task account
  install-ssh      Install over strict OpenSSH using a key/SSH agent; root or passwordless sudo is required
  uninstall-local  Remove the standard Agent service; preserve task history and managed applications
  agent-info       Read the local Agent's last confirmed identity, wait reason and active intent
  backup           Export a checksum-verified logical backup from the quorum-holding controller
  restore          Restore into a new directory; old controllers must be externally fenced first
  pki-init         Generate a private CA directory using built-in Rust cryptography
  pki-issue        Issue a client or server identity in a new private directory
  fetch            Download and verify a content-addressed artifact using bounded memory
  run-script       Execute an explicitly authorized script with a durable, non-replaying receipt (lab only)
  api-schema       Write the exact embedded OpenAPI contract to stdout
  help             Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin migrate-agent`

```text
Usage: converyn admin migrate-agent [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin output-key`

```text
Usage: converyn admin output-key [OPTIONS] --file <FILE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --file <FILE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --rotate
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin init`

```text
Usage: converyn admin init [OPTIONS] --directory <DIRECTORY> --cluster <CLUSTER>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --directory <DIRECTORY>

      --cluster <CLUSTER>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --member <MEMBERS>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --target <TARGETS>

      --leaf-lifetime-seconds <LEAF_LIFETIME_SECONDS>
          Lifetime for initial leaf certificates and the online Node issuer, in seconds (300..31536000) [default: 2592000]
  -h, --help
          Print help
```

### `admin initialize`

```text
Usage: converyn admin initialize [OPTIONS] --members <MEMBERS>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --members <MEMBERS>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin bootstrap`

```text
Usage: converyn admin bootstrap [OPTIONS] --node <NODE> --output <OUTPUT>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --node <NODE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --output <OUTPUT>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --ttl-seconds <TTL_SECONDS>      [default: 900]
      --recover
  -h, --help                           Print help
```

### `admin enroll`

```text
Usage: converyn admin enroll [OPTIONS] --bootstrap <BOOTSTRAP> --state-dir <STATE_DIR> --output <OUTPUT>

Options:
      --bootstrap <BOOTSTRAP>

      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --state-dir <STATE_DIR>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --output <OUTPUT>

      --template <TEMPLATE>

      --hardware-id-file <HARDWARE_ID_FILE>

  -h, --help
          Print help
```

### `admin renew`

```text
Usage: converyn admin renew [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin install-local`

```text
Usage: converyn admin install-local [OPTIONS]

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --hardware-id-file <HARDWARE_ID_FILE>

      --bootstrap <BOOTSTRAP>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --bootstrap-stdin

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help
          Print help
```

### `admin install-ssh`

```text
Usage: converyn admin install-ssh [OPTIONS] --host <HOST> --user <USER> --known-hosts <KNOWN_HOSTS> --bootstrap <BOOTSTRAP>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --hardware-id-file <HARDWARE_ID_FILE>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --host <HOST>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --user <USER>

      --port <PORT>
          [default: 22]
      --identity-file <IDENTITY_FILE>

      --known-hosts <KNOWN_HOSTS>

      --binary <BINARY>

      --bootstrap <BOOTSTRAP>

  -h, --help
          Print help
```

### `admin uninstall-local`

```text
Usage: converyn admin uninstall-local [OPTIONS] --node <NODE>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --node <NODE>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin agent-info`

```text
Usage: converyn admin agent-info [OPTIONS] --state-dir <STATE_DIR>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --state-dir <STATE_DIR>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin backup`

```text
Usage: converyn admin backup [OPTIONS] --output <OUTPUT>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --output <OUTPUT>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin restore`

```text
Usage: converyn admin restore [OPTIONS] --backup <BACKUP> --data-dir <DATA_DIR> --node-id <NODE_ID>

Options:
      --backup <BACKUP>
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --data-dir <DATA_DIR>
      --node-id <NODE_ID>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --old-control-plane-fenced
      --keyring <KEYRING>
  -h, --help                           Print help
```

### `admin pki-init`

```text
Usage: converyn admin pki-init [OPTIONS] --directory <DIRECTORY> --name <NAME>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --directory <DIRECTORY>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --name <NAME>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `admin pki-issue`

```text
Usage: converyn admin pki-issue [OPTIONS] --authority <AUTHORITY> --directory <DIRECTORY> --name <NAME>

Options:
      --authority <AUTHORITY>

      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --directory <DIRECTORY>

      --name <NAME>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --ip <IP>

      --leaf-lifetime-seconds <LEAF_LIFETIME_SECONDS>
          Leaf certificate lifetime in seconds (300..31536000), capped by the CA expiry [default: 2592000]
  -h, --help
          Print help
```

### `admin fetch`

```text
Usage: converyn admin fetch [OPTIONS] --url <URL> --sha256 <SHA256> --size <SIZE> --cache-dir <CACHE_DIR>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --url <URL>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --sha256 <SHA256>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --size <SIZE>

      --cache-dir <CACHE_DIR>

      --attempts <ATTEMPTS>
          [default: 3]
      --bytes-per-second <BYTES_PER_SECOND>

  -h, --help
          Print help
```

### `admin run-script`

```text
Usage: converyn admin run-script [OPTIONS] --state-dir <STATE_DIR> --operation-id <OPERATION_ID> --script <SCRIPT> --sha256 <SHA256>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --state-dir <STATE_DIR>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --operation-id <OPERATION_ID>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --script <SCRIPT>
      --sha256 <SHA256>
      --timeout <TIMEOUT>              [default: 60]
  -h, --help                           Print help
```

### `admin api-schema`

```text
Usage: converyn admin api-schema [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `task`

Workflow script input/output helpers

```text
Usage: converyn task [OPTIONS] <COMMAND>

Commands:
  emit     Merge a JSON object from stdin into this managed task's private result file
  invoke   Invoke the registered local script with explicitly bound JSON context inputs
  context  Read a field from the current task context, including this task's own updates
  help     Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `task emit`

```text
Usage: converyn task emit [OPTIONS] --stdin

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --stdin
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --context-update
      --key <KEY>
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

### `task invoke`

```text
Usage: converyn task invoke [OPTIONS] --script <SCRIPT>

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --environment-json-env <ENVIRONMENTS>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --fixed-json-env <FIXED>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --script <SCRIPT>

      --publish-context

  -h, --help
          Print help
```

### `task context`

```text
Usage: converyn task context [OPTIONS] --get <GET>

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --get <GET>
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
      --raw
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

## `server`

Start a controller

```text
Usage: converyn server [OPTIONS]

Options:
      --config <CONFIG>
          Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --id <ID>

      --context <CONTEXT>
          Select a context for this invocation without changing the saved selection
      --listen <LISTEN>

      --data-dir <DATA_DIR>

  -o, --output-format <OUTPUT_FORMAT>
          Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
      --cluster <CLUSTER>
          [default: converyn-p0]
      --token <TOKEN>
          [env: CONVERYN_TOKEN] [default: ""]
      --auth-file <AUTH_FILE>

      --issuer-cert <ISSUER_CERT>

      --issuer-key <ISSUER_KEY>

      --leaf-lifetime-seconds <LEAF_LIFETIME_SECONDS>
          [default: 2592000]
      --tls-cert <TLS_CERT>

      --tls-key <TLS_KEY>

      --tls-ca <TLS_CA>

  -h, --help
          Print help
```

## `agent`

Start an autonomous Agent

```text
Usage: converyn agent [OPTIONS]

Options:
      --config <CONFIG>                Config file. API default: CONVERYN_CONFIG or ~/.converyn/config.json; server/agent/renew require their own explicit profile
      --context <CONTEXT>              Select a context for this invocation without changing the saved selection
  -o, --output-format <OUTPUT_FORMAT>  Output format: human-readable tables/details, or indented JSON for scripts [default: table] [possible values: table, json, wide]
  -h, --help                           Print help
```

