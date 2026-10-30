#!/bin/sh
set -eu
test -s /opt/converyn-demo/config
printf "Verified version=%s\n" "$VERSION"
printf "Node port=%s region=%s\n" "$PORT" "$REGION"
printf "Workflow values=%s\n" "$WORKFLOW_VALUES_JSON"
