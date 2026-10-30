#!/usr/bin/env python3
"""Reject product-version drift before verification or a tagged release."""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def check(root, tag=None, binary=None):
    def value(text, key):
        match = re.search(rf'(?m)^{re.escape(key)}\s*=\s*"([^"]+)"\s*$', text)
        if match is None:
            raise ValueError(f'missing TOML string: {key}')
        return match[1]

    workspace = (root / 'Cargo.toml').read_text().split('[workspace.package]', 1)[1].split('\n[', 1)[0]
    version = value(workspace, 'version')
    number = r'(?:0|[1-9][0-9]*)'
    identifier = r'(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
    pattern = rf'{number}\.{number}\.{number}(?:-{identifier}(?:\.{identifier})*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?'
    if not re.fullmatch(pattern, version):
        raise ValueError('workspace version must be a semantic version')
    package = json.loads((root / 'web/package.json').read_text())
    lock = json.loads((root / 'web/package-lock.json').read_text())
    api = json.loads((root / 'docs/schema/openapi.json').read_text())
    # Cargo writes these scalar fields in separate package tables; avoid adding
    # a TOML parser dependency to the existing Python development tools.
    cargo = (root / 'Cargo.lock').read_text().split('[[package]]')[1:]
    versions = {
        'web/package.json': package['version'],
        'web/package-lock.json': lock['version'],
        'web/package-lock.json packages root': lock['packages']['']['version'],
        'OpenAPI': api['info']['version'],
    }
    for path in [*root.glob('crates/*/Cargo.toml'), *root.glob('plugins/*/Cargo.toml')]:
        crate = path.read_text().split('[package]', 1)[1].split('\n[', 1)[0]
        name = value(crate, 'name')
        if not re.search(r'(?m)^version\.workspace\s*=\s*true\s*$', crate):
            raise ValueError(f'{path.relative_to(root)} must inherit the workspace version')
        entries = [p for p in cargo if value(p, 'name') == name and not re.search(r'(?m)^source\s*=', p)]
        if len(entries) != 1:
            raise ValueError(f'Cargo.lock must contain one workspace package: {name}')
        versions['Cargo.lock ' + name] = value(entries[0], 'version')
    if tag is not None:
        versions['Git tag'] = tag.removeprefix('v') if tag.startswith('v') else ''
    if binary is not None:
        versions['binary'] = subprocess.check_output([str(binary.resolve()), '--version'], text=True).strip().removeprefix('converyn ')
    for source, actual in versions.items():
        if actual != version:
            raise ValueError(f'{source} version {actual!r} differs from workspace {version!r}')
    return version


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', help='require an exact v-prefixed version tag')
    parser.add_argument('--binary', type=Path, help='also verify an executable built for this host')
    args = parser.parse_args()
    try:
        print(check(ROOT, args.tag, args.binary))
    except (ValueError, KeyError) as error:
        parser.exit(1, f'Version check failed: {error}\n')
