#!/bin/bash
set -euo pipefail
python3 - <<'PY'
import json,urllib.request,shutil,time
end = time.monotonic() + 15
while True:
    try:
        assert json.load(urllib.request.urlopen('http://127.0.0.1:18080/ready', timeout=2))['ready']
        break
    except OSError:
        if time.monotonic() >= end:
            raise
        time.sleep(.1)
assert shutil.disk_usage('/opt/converyn-node-demo/data').free > 1024**3
print('HTTP service ready; disk headroom verified')
PY
