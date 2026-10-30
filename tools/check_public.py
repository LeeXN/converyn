#!/usr/bin/env python3
"""Reject private lab dependencies before a source checkout or release is shared."""
from pathlib import Path
import re
import sys
import argparse
import hashlib
import json

ROOT = Path(__file__).resolve().parents[1]
PRIVATE = re.compile(r'\bgrapple\b|\bcybertron\b|\bblitzwing\b|10\.121\.214\.', re.I)
DEPENDENCY = re.compile(r'(?:include_str!|include_bytes!|source\s|importlib|copytree|read_text|read_bytes).{0,160}["\'][^"\'\n]*\.local/')
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release', type=Path, help='also verify one generated release directory')
args = parser.parse_args()
errors = []
for base in ['crates', 'plugins', 'docs', 'example', 'scripts', 'tools', 'web', '.github']:
    for path in (ROOT / base).rglob('*'):
        if not path.is_file() or any(part in path.parts for part in ['__pycache__', 'node_modules', 'dist', 'target', '.vite']) or path.suffix in ('.pyc', '.png'):
            continue
        if path == Path(__file__).resolve() or path == ROOT / 'tools/check_docs.py':
            continue
        text = path.read_text(errors='replace')
        if PRIVATE.search(text) or DEPENDENCY.search(text):
            errors.append(str(path.relative_to(ROOT)))
for path in [*ROOT.glob('README*'), *ROOT.glob('CONTRIBUTING*')]:
    if PRIVATE.search(path.read_text()):
        errors.append(path.name)
if errors:
    print('Private content/dependency in public sources: ' + ', '.join(errors), file=sys.stderr)
    sys.exit(1)
print('Public source boundary verified')

if args.release:
    release = args.release.resolve()
    manifest = json.loads((release / 'manifest.json').read_text())
    assert not PRIVATE.search((release / 'manifest.json').read_text()), 'Private release metadata'
    expected = {manifest['binary'], 'install.sh', 'init.sh', 'manifest.json'}
    assert {p.name for p in release.iterdir()} == expected, 'Unexpected release contents'
    for name, digest in [(manifest['binary'], manifest['binary_sha256']), ('install.sh', manifest['installer_sha256']), ('init.sh', manifest['initializer_sha256'])]:
        path = release / name
        assert path.is_file() and not path.is_symlink(), name
        data = path.read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, 'Release digest differs: ' + name
        assert path.stat().st_mode & 0o111, 'Release file is not executable: ' + name
        assert not re.search(PRIVATE.pattern.encode(), data, re.I), 'Private content in release: ' + name
    print('Release files, hashes, executable modes and private-content boundary verified')
