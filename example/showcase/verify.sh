#!/usr/bin/env bash
set -euo pipefail
test "$MESSAGE" = ready
test -n "$APPLICATION_TOKEN"
test "$(cat /opt/converyn-showcase/package.txt)" = 'Converyn showcase package'
curl --fail --silent http://127.0.0.1:18081/ | grep -F '"version":"v1"'
printf 'Verified package, service, file content, output binding and Secret presence\n'
