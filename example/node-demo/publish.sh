#!/usr/bin/env bash
set -euo pipefail
python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import tempfile

version = os.environ['VERSION']
if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,63}', version):
    raise SystemExit('VERSION must be a simple release name, at most 64 characters')
root = Path('/opt/converyn-node-demo/data')
releases = root / 'releases'
assert not releases.is_symlink(), 'unsafe release root'
releases.mkdir(mode=0o755, exist_ok=True)
releases.chmod(0o755)
release = releases / version
body = ('<h1>Converyn demo ' + version + '</h1>\n').encode()
metadata = {'version': version, 'node': socket.gethostname(),
            'sha256': hashlib.sha256(body).hexdigest()}
if release.exists():
    assert release.is_dir() and not release.is_symlink(), 'unsafe release directory'
    assert (release / 'index.html').read_bytes() == body, 'release content conflict'
    assert json.loads((release / 'release.json').read_text()) == metadata, 'release metadata conflict'
else:
    stage = Path(tempfile.mkdtemp(prefix=version + '-', dir=releases))
    for name, data in [('index.html', body), ('release.json', json.dumps(metadata).encode())]:
        with (stage / name).open('wb') as output:
            output.write(data)
            output.flush()
            os.fchmod(output.fileno(), 0o644)
            os.fsync(output.fileno())
    def sync(directory):
        descriptor = os.open(directory, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    stage.chmod(0o755)
    sync(stage)
    stage.rename(release)
    sync(releases)
# The service reads as nobody while deployment scripts run with host authority.
# Normalize only these validated, immutable demo files, including an older release.
release.chmod(0o755)
for name in ('index.html', 'release.json'):
    path = release / name
    assert path.is_file() and not path.is_symlink(), 'unsafe release file'
    path.chmod(0o644)
link = root / ('current.next.' + str(os.getpid()))
link.symlink_to('releases/' + version)
os.replace(link, root / 'current')
descriptor = os.open(root, os.O_RDONLY)
try:
    os.fsync(descriptor)
finally:
    os.close(descriptor)
print('Published ' + version)
PY
