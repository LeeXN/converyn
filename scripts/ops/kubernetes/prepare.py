#!/usr/bin/env python3
"""Fetch reviewed, digest-pinned community inputs; never touches a target host."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).resolve().parents[3] / "example/kubernetes"


def digest(path):
    sha = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            sha.update(block)
    return sha.hexdigest()


def prepare(directory, images_archive=None):
    directory.mkdir(parents=True, exist_ok=True)
    lock = json.loads((ROOT / 'sources.lock.json').read_text())
    downloads = directory / 'downloads'
    blobs = directory / 'blobs'
    downloads.mkdir(exist_ok=True)
    blobs.mkdir(exist_ok=True)

    def fetch(item):
        path = downloads / item['name']
        if not path.exists() or digest(path) != item['sha256']:
            partial = path.with_suffix('.part')
            subprocess.run(['curl', '--fail', '--location', '--silent', '--show-error',
                            '--retry', '3', '--connect-timeout', '15', '--max-time', '600',
                            '--proto', '=https', '--proto-redir', '=https',
                            '--output', str(partial), item['url']], check=True)
            if digest(partial) != item['sha256']:
                partial.unlink()
                raise ValueError('upstream checksum mismatch: ' + item['name'])
            partial.replace(path)
        return item, path

    records = []
    for item, path in ThreadPoolExecutor(4).map(fetch, lock['sources']):
        targets = item.get('members')
        if targets:
            with tarfile.open(path) as archive:
                for member, target in targets.items():
                    info = archive.getmember(member)
                    if not info.isfile():
                        raise ValueError('archive member must be a regular file')
                    temporary = downloads / (Path(target).name + '.extracted')
                    with archive.extractfile(info) as source, temporary.open('wb') as output:
                        shutil.copyfileobj(source, output)
                    records.append(record(blobs, temporary, target, item))
        else:
            records.append(record(blobs, path, item['target'], item))
    if images_archive:
        if images_archive.stat().st_size > 10 * 1024**3:
            raise ValueError('image archive exceeds the runtime artifact limit of 10 GiB')
        with tarfile.open(images_archive) as archive:
            if not {'manifest.json', 'index.json'} & set(archive.getnames()):
                raise ValueError('expected a Docker/OCI image archive')
        records.append(record(blobs, images_archive, '/opt/converyn-example/images.tar',
                              {'url': 'operator://reviewed-image-archive', 'sha256': digest(images_archive)}))
    manifest = {'versions': lock['versions'], 'artifacts': records}
    path = directory / 'artifacts.json'
    content = json.dumps(manifest, indent=2) + '\n'
    if path.exists() and path.read_text() != content:
        raise ValueError('use a new output directory for changed inputs')
    path.write_text(content)
    print('Verified', len(records), 'artifacts:', path)


def record(blobs, path, target, source):
    sha = digest(path)
    destination = blobs / sha
    if not destination.exists():
        shutil.copyfile(path, destination)
    elif digest(destination) != sha:
        raise ValueError('corrupt existing blob: ' + sha)
    return {'target': target, 'sha256': sha, 'size': path.stat().st_size,
            'mode': 0o755 if target.startswith(('/usr/local/', '/opt/cni/bin/')) else 0o644,
            'source_url': source['url'], 'source_sha256': source['sha256']}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--images-archive', type=Path, help='optional reviewed Docker/OCI archive for disconnected nodes')
    args = parser.parse_args()
    prepare(args.output.resolve(), args.images_archive)
