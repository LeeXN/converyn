#!/usr/bin/env bash
set -euo pipefail
printf '{"message":"ready"}' | "$CONVERYN_EXECUTABLE" task emit --stdin
