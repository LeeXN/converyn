#!/usr/bin/env bash
set -euo pipefail
python3 - <<'PY'
import hashlib
import json
import os
import urllib.request

health = json.load(urllib.request.urlopen('http://127.0.0.1:18080/health', timeout=5))
body = urllib.request.urlopen('http://127.0.0.1:18080/', timeout=5).read()
assert health['version'] == os.environ['VERSION'], 'served version differs'
assert health['sha256'] == hashlib.sha256(body).hexdigest(), 'served content digest differs'
print(json.dumps(health, sort_keys=True))
PY
