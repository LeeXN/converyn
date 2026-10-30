#!/usr/bin/env bash
set -euo pipefail
curl --fail --silent --show-error --retry 20 --retry-connrefused --retry-delay 1 --max-time 5 http://127.0.0.1:18081/ >/dev/null
