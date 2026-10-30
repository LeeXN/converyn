#!/usr/bin/env python3
"""Check the offline image inventory against the pinned kubeadm and runtime config."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3] / "example/kubernetes"


def validate_images(required, sandbox, inventory):
    pauses = [image for image in required if image.startswith('registry.k8s.io/pause:')]
    if pauses != [sandbox]:
        raise ValueError('containerd sandbox differs from pinned kubeadm: '
                         + sandbox + ' vs ' + ', '.join(pauses))
    available = {row.get('archive_reference', row['reference']) for row in inventory['images']}
    missing = sorted(set(required) - available)
    if missing:
        raise ValueError('offline inventory is missing required images: ' + ', '.join(missing))
    return {'sandbox_image': sandbox, 'kubeadm_images': required,
            'inventory_images': len(available)}


def check(kubeadm, inventory, containerd_config):
    lock = json.loads((ROOT / 'sources.lock.json').read_text())
    source = next(item for item in lock['sources'] if item['name'] == 'kubeadm')
    digest = hashlib.sha256()
    with kubeadm.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    if digest.hexdigest() != source['sha256']:
        raise ValueError('kubeadm SHA256 differs from sources.lock.json')
    matches = re.findall(r'''^\s*sandbox\s*=\s*(['"])([^'"]+)\1\s*(?:#.*)?$''',
                         containerd_config.read_text(), re.M)
    if len(matches) != 1:
        raise ValueError('expected exactly one containerd sandbox image pin')
    # Prepared downloads remain private/non-executable; run an isolated copy.
    with tempfile.TemporaryDirectory(prefix='converyn-image-inventory-') as directory:
        binary = Path(directory) / 'kubeadm'
        shutil.copyfile(kubeadm, binary)
        binary.chmod(0o700)
        version = subprocess.check_output([str(binary), 'version', '-o', 'short'], text=True).strip()
        if version != lock['versions']['kubernetes']:
            raise ValueError('kubeadm version differs from sources.lock.json: ' + version)
        # An explicit fixed version avoids kubeadm's remote release lookup.
        required = subprocess.check_output([str(binary), 'config', 'images', 'list',
                                           '--kubernetes-version', version], text=True).splitlines()
    result = validate_images(required, matches[0][1], json.loads(inventory.read_text()))
    return {'kubernetes_version': version, **result}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kubeadm', required=True, type=Path,
                        help='downloaded kubeadm matching sources.lock.json')
    parser.add_argument('--inventory', required=True, type=Path,
                        help='reviewed archive images.json with reference/archive_reference entries')
    parser.add_argument('--containerd-config', type=Path,
                        default=ROOT / 'configs/host/containerd/config.toml')
    args = parser.parse_args()
    try:
        result = check(args.kubeadm, args.inventory, args.containerd_config)
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, str(error) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
