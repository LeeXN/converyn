#!/bin/bash
set -euo pipefail
test "$MESSAGE" = ready
test -n "$TOKEN"
printf 'Declared message: %s; credential received\n' "$MESSAGE"
