#!/usr/bin/env python3
"""Build an inspectable demonstration package; never install on any host."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

import yaml

ROOT = Path(__file__).resolve().parents[3]


def prepare(output, manager):
    output = output.resolve()
    if output.exists():
        raise ValueError('output exists; use a new directory')
    tool = 'dpkg-deb' if manager == 'deb' else 'rpmbuild'
    if not shutil.which(tool):
        raise ValueError('install the operator build dependency first: ' + tool)
    with tempfile.TemporaryDirectory(prefix='converyn-showcase-') as temporary:
        root = Path(temporary)
        payload = root / 'package/opt/converyn-showcase'
        payload.mkdir(parents=True)
        (payload / 'package.txt').write_text('Converyn showcase package\n')
        if manager == 'deb':
            control = root / 'package/DEBIAN'
            control.mkdir()
            (control / 'control').write_text('Package: converyn-showcase\nVersion: 1.0.0-1\nArchitecture: all\nMaintainer: Converyn contributors\nDescription: Isolated Converyn demonstration data\n')
            subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(root / 'package'), str(root / 'demo.deb')], check=True)
            package, version = root / 'demo.deb', '1.0.0-1'
        else:
            for directory in ['BUILD', 'BUILDROOT', 'RPMS', 'SOURCES', 'SPECS', 'SRPMS']:
                (root / directory).mkdir()
            spec = root / 'SPECS/demo.spec'
            spec.write_text('''Name: converyn-showcase
Version: 1.0.0
Release: 1
Summary: Isolated Converyn demonstration data
License: MIT
BuildArch: noarch
%description
A single demonstration data file. No package scripts or service changes.
%install
mkdir -p %{buildroot}/opt/converyn-showcase
printf 'Converyn showcase package\\n' > %{buildroot}/opt/converyn-showcase/package.txt
%files
/opt/converyn-showcase/package.txt
''')
            subprocess.run(['rpmbuild', '--define', '_topdir ' + str(root), '-bb', str(spec)], check=True)
            package = root / 'RPMS/noarch/converyn-showcase-1.0.0-1.noarch.rpm'
            version = '0:1.0.0-1.noarch'
        shutil.copytree(ROOT / 'example/showcase', output)
        assets = output / 'assets'
        assets.mkdir()
        filename = 'converyn-showcase.' + manager
        shutil.copy2(package, assets / filename)
        workflow = yaml.safe_load((output / 'workflow.yaml').read_text())
        operation = next(s['operation'] for s in workflow['playbook']['steps'] if s['operation']['kind'] == 'package')
        operation.update(manager=manager, version=version, artifact={'$artifact': 'assets/' + filename})
        (output / 'workflow.yaml').write_text(yaml.safe_dump(workflow))
        (output / 'build.json').write_text(json.dumps({'manager': manager, 'name': 'converyn-showcase', 'version': version}, indent=2) + '\n')
    print('Prepared workflow:', output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manager', choices=['deb', 'rpm'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    prepare(args.output, args.manager)
