#!/bin/bash
set -euo pipefail
# Values are JSON data. They are never evaluated as shell source.
token=$(head -c 24 /dev/urandom | base64 | tr -d '\n')
printf '{"message":"ready","token":"%s"}' "$token" |
    "$CONVERYN_EXECUTABLE" task emit --stdin
