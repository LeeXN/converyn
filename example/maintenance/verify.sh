#!/usr/bin/env bash
set -euo pipefail
test -r /proc/sys/kernel/random/boot_id
printf 'Recovered on kernel %s; boot ID %s\n' "$(uname -r)" "$(cat /proc/sys/kernel/random/boot_id)"
