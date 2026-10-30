# Step outputs and configuration updates

[English](outputs.md) | [简体中文](../../zh-CN/how-to/outputs.md)

Declare typed `outputs` on a producer and `inputs` on a later consumer. This supports generated join commands, tokens and discovered application configuration in one Run. As with a registered Ansible result, the producer's committed result is the input; Converyn additionally freezes input commitments and binds secret delivery to the original Node grant. See the complete [example](../../../example/outputs/README.md).

## Publish and consume

```yaml
outputs:
  join_command:
    type: string
    secret: true
    max_bytes: 4096
    ttl_seconds: 3600
```

```bash
printf '%s' "$join_command" |
  "$CONVERYN_EXECUTABLE" task emit --stdin --key join_command
```

```yaml
inputs:
  /environment/JOIN_COMMAND:
    step: create-token
    name: join_command
```

`task emit --stdin` accepts a JSON object of named values; `--key NAME` wraps raw stdin as one string. It merges into the private `CONVERYN_OUTPUT_FILE` created by the supervisor. Never source that file. Values are data, not executable template substitutions. Every declared field is required; undeclared fields, wrong types, unsafe ownership/permissions, malformed JSON and swallowed writer errors fail the task even after exit 0. Only successful, verified original receipts publish consumable outputs. Manual resolution cannot invent a successful producer result.

A reference may specify `node`, `batch`, an RFC 6901 `pointer` and `collect`. A per-node producer defaults to the same node; a once-per-run producer must resolve uniquely. References must point to an earlier wave. `collect: true` returns ordered `{node_uid,batch,value}` objects. Ambiguous, missing, future and conflicting bindings are rejected. A consumer needs an explicit source Node when selecting a different per-node producer.

Public values can bind the existing permitted data fields: script environment, file `/content`, service/HTTP options and plugin spec fields. Input bindings cannot change resource keys, file destinations, script identity, execution profile, artifact identity or target identity. Binary/package artifacts remain fixed. File text gets a new immutable definition and hash; `.json` destinations require valid JSON. Native/plugin public outputs select verified receipt evidence with `source` JSON pointers. Secret output producers are scripts; native evidence is public evidence, not a secret channel.

## Secrets, logs and keys

Secret values are sealed by the Agent with the Run's P-256 public key, ECDH, HKDF-SHA256 and ChaCha20-Poly1305; Run, Work and output name are authenticated. Only ciphertext enters replicated state, history and state backups. No public plaintext digest is recorded. Secret references may bind only script environment fields. Only the assigned, currently authenticated Node with the committed consumer grant can fetch plaintext through the existing mTLS API. Human administrators cannot use that endpoint to read outputs. Responses use `Cache-Control: no-store` and are excluded from API evidence recordings.

Declaring any secret output suppresses the producer's entire stdout/stderr, including live upload, receipt tails and download/export. Producer text is not parsed for results. Consumers receive redaction patterns for known values. Scripts remain responsible for business files and external commands they explicitly invoke; use private permissions for application credentials.

`admin init` creates an independent `output-keys.json` and copies it into each controller directory; each server profile has `output_keyring`. Existing installations must generate a private keyring and install the same retained key set on all controllers before running secret-output workflows:

```bash
converyn admin output-key --file /secure/output-keys.json
converyn admin output-key --file /secure/output-keys.json --rotate
```

The command prints only the public descriptor. Distribute one rotated complete keyring atomically, retaining old keys; do not independently rotate each controller. Keep its offline recovery copy separately from state backups. Missing old private keys prevent consumption of old ciphertext. Rotation does not invalidate existing Run results.

TTL starts at the producer's first committed successful completion. It is checked during materialization, new grant authorization and private delivery. Expired inputs fail the consumer and release Run locks; expired delivery returns 410. Missing keys return retryable 503. Invalid ciphertext, pointer/type or aggregate size returns permanent 422. A retry of the same accepted grant retains its original authorization; it never creates a replacement execution or extends the secret lifetime.

## Context-oriented scripts

For a script needing many generated configuration fields, bind a declared object input to an environment variable and invoke its immutable script with the local helper:

```bash
"$CONVERYN_EXECUTABLE" task invoke \
  --environment-json-env INITIAL_CONTEXT \
  --environment-json-env PREVIOUS_CONTEXT \
  --fixed-json-env NODE_METADATA --publish-context --script ./business.sh
```

Inside `business.sh`:

```bash
printf '{"discovered_address":"192.0.2.20"}' |
  "$CONVERYN_EXECUTABLE" task emit --stdin --context-update
"$CONVERYN_EXECUTABLE" task context --get discovered_address --raw
```

Context updates run inside `business.sh`. Initial/later contexts overlay in order; collected parallel contexts reject different values for the same key. Fixed metadata applies last to the child environment and is excluded from published context. Protected loader/platform variables and oversized environments are rejected. The helper never changes UID, cgroup, filesystem grants or script identity. It has no network service or personal OP credential. `--publish-context` requires a declared `context` object output; protect it whenever it contains credentials.

Context exists only in this Run's declared output graph. It does not mutate Workflow revisions or global cluster variables. A fresh Run starts from its declared initial inputs. Persistent application configuration uses an explicit native file/plugin operation. Workflow groups and results stay local to the workflow. Use declared outputs to pass business results; operations cannot write global node roles.

## Recovery and limits

Input materialization is committed before claim. The original Plan stays immutable; `Attempt.materialized` records the effective Work, input hash and producer commitments. Claim must echo that input hash. Leader changes, retries and restart use the frozen input and original receipt. Completion is persisted before plaintext result files are removed. Unknown results are quarantined privately and never become later inputs.

Limits: 128 output fields; at most 16 KiB per field; 64 KiB declared document; 256 KiB receipt; 4 MiB declared output budget per Run; secret TTL 1–86400 seconds; at most 128 KiB resolved environment. Default field limit is 4096 bytes and TTL 3600 seconds. Workflow dataflow runs execute once; continuous reconciliation cannot declare Run outputs/inputs. Expiration requires a fresh deliberate Run, not an automatic replay of non-idempotent work.
