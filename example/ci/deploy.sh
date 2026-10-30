#!/usr/bin/env bash
# The same commands are used by all three sample pipelines.
set +x
set -euo pipefail
: "${CONVERYN_CONFIG:?Private runner config path is required}"
: "${CONVERYN_WORKFLOW_DIR:?Workflow source directory is required}"
: "${CONVERYN_WORKFLOW_UID:?Workflow UID is required}"
: "${CONVERYN_TARGET:?Target preset is required}"
: "${DEPLOYMENT_ID:?Stable CI execution identifier is required}"
: "${RELEASE_VERSION:?Version to deploy or restore is required}"
parameter=${RELEASE_PARAMETER:-VERSION}
[[ $parameter =~ ^[A-Za-z_][A-Za-z0-9_.-]*$ ]] || { echo 'Invalid release parameter' >&2;exit 2; }
[[ -f $CONVERYN_CONFIG && ! -L $CONVERYN_CONFIG && $(stat -c %a "$CONVERYN_CONFIG") == 600 ]] || { echo 'Expected private runner config mode 0600' >&2;exit 2; }
cli=(converyn --config "$CONVERYN_CONFIG")
[[ -z ${CONVERYN_CONTEXT:-} ]] || cli+=(--context "$CONVERYN_CONTEXT")
request=ci-$(printf '%s\n' "$DEPLOYMENT_ID" "$CONVERYN_WORKFLOW_UID" "$CONVERYN_TARGET" "$parameter=$RELEASE_VERSION" "${RELEASE_IMAGE:-}" | sha256sum | cut -c 1-40)
overrides=(--set "$parameter=$RELEASE_VERSION")
[[ -z ${RELEASE_IMAGE:-} ]] || overrides+=(--set "image=$RELEASE_IMAGE")
converyn validate workflows --file "$CONVERYN_WORKFLOW_DIR"
"${cli[@]}" register workflows --file "$CONVERYN_WORKFLOW_DIR"
"${cli[@]}" run workflows "$CONVERYN_WORKFLOW_UID" --target "$CONVERYN_TARGET" "${overrides[@]}" --dry-run
"${cli[@]}" run workflows "$CONVERYN_WORKFLOW_UID" --target "$CONVERYN_TARGET" "${overrides[@]}" --request-id "$request" --wait --timeout-seconds 1800
"${cli[@]}" get runs "run-$request"
# This prints task evidence, never the runner credential files.
"${cli[@]}" logs runs "run-$request"
