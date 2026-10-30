#!/usr/bin/env python3
"""Compile a cluster-bound Kubernetes removal workflow; never register or run it."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess

import yaml

import render

ROOT = Path(__file__).resolve().parents[3] / "example/kubernetes"


def render_uninstall(installation, binary, output, resume_from=None):
    installation, output = Path(installation).resolve(), Path(output).resolve()
    config = json.loads((installation / 'public-cluster.json').read_text())
    control = render.validate(config)
    kit = installation / 'private-kubeadm'
    manifest = json.loads((kit / 'manifest.json').read_text())
    ca = hashlib.sha256((kit / 'ca.crt').read_bytes()).hexdigest()
    if manifest['ca_sha256'] != ca or manifest['endpoint'] != config['endpoint']:
        raise ValueError('installation CA/endpoint does not match the original private kit')
    source_file = installation / 'plans/00-install-kubernetes.input.json'
    if not source_file.exists():
        source_file = installation / 'plans/02-control-plane.input.json'
    source = json.loads(source_file.read_text())
    if source['hosts']['node_group_uid'] != config['node_group_uid'] or source['owner_uid'] != config['owner_uid']:
        raise ValueError('installation cluster/owner does not match its compiled input')
    expected = {n['uid'] for n in config['nodes']}
    if set(source['hosts']['node_uids']) != expected:
        raise ValueError('installation inventory differs from the public configuration')
    if resume_from is not None and resume_from not in expected:
        raise ValueError('resume Node UID is outside the original installation')
    kubeadm = next(s['operation']['environment']['BINARY_SHA256'] for s in source['playbook']['steps']
                   if s['operation']['kind'] == 'exec' and 'BINARY_SHA256' in s['operation'].get('environment', {}))
    if not re.fullmatch('[0-9a-f]{64}', kubeadm):
        raise ValueError('installation has no pinned kubeadm binary')
    if output.exists():
        raise ValueError('output already exists; preserve previous plans and evidence')
    output.mkdir(parents=True, mode=0o700)
    body = (ROOT / 'scripts/uninstall.sh').read_text()
    digest = render.sha(body)
    render.write(output / 'identity.json', {'cluster_uid': config['cluster_uid'], 'owner_uid': config['owner_uid'],
        'endpoint': config['endpoint'], 'ca_sha256': ca, 'nodes': config['nodes'], 'installation': str(installation),
        'resume_from': resume_from,
        'preserve': ['NFS/PV data', 'containerd and image cache', 'private installation kit',
                     'Converyn credentials, services, state, receipts and run history'],
        'retained_host_configuration': ['kubelet unit and binaries (disabled)', 'sysctl and kernel modules',
            'kube-proxy/Calico firewall rules and interfaces (no blanket network cleanup)', 'API proxy and NFS server']})
    definitions = output / 'definitions'
    definitions.mkdir()
    (definitions / (digest + '.txt')).write_text(body)
    steps = []

    controls_remaining = sum(n['control_plane'] for n in config['nodes'])
    def add(action, node, phase):
        nonlocal controls_remaining
        if action in ('reset', 'resume') and node['control_plane']:
            controls_remaining -= 1
        steps.append({'id': 'uninstall-' + action + '-' + node['uid'], 'phase': phase,
            'scope': 'per_node', 'hosts': 'all[' + str(config['nodes'].index(node)) + ']', 'max_parallel': 1,
            'operation': {'kind': 'exec', 'script_sha256': digest, 'resources': ['kubernetes-host'],
                'execution': 'host', 'timeout_seconds': 1200,
                'environment': {'UNINSTALL_ACTION': action, 'CA_SHA256': ca, 'BINARY_SHA256': kubeadm,
                    'CLUSTER_ENDPOINT': config['endpoint'], 'NODE_NAME': node['hostname'],
                    'NODE_ADDRESS': node['address'], 'NODE_UID': node['uid'],
                    'FIRST_CONTROL': 'true' if node == control else 'false',
                    'ETCD_OBSERVER': control['hostname'],
                    'EXPECTED_ETCD_MEMBERS': str(controls_remaining) if action in ('reset', 'resume') and node['control_plane'] and node != control else '0',
                    'KUBECONFIG': '/etc/kubernetes/admin.conf' if node['control_plane'] else '/etc/kubernetes/kubelet.conf'}}})

    if resume_from is None:
        for node in config['nodes']:
            add('check', node, 'prepare')
        for node in config['nodes']:
            add('backup', node, 'prepare')
    ordered = ([n for n in config['nodes'] if not n['control_plane']] +
               [n for n in config['nodes'] if n['control_plane'] and n != control] + [control])
    started = resume_from is None
    for node in ordered:
        started = started or node['uid'] == resume_from
        if not started:
            if node['control_plane']:
                controls_remaining -= 1
            continue
        add('resume' if resume_from else 'reset', node, 'rollout')
    name = '90-uninstall-kubernetes'
    playbook = {'api_version': 'converyn/v1', 'name': name, 'steps': steps}
    # One batch preserves playbook waves even when inventory lists controls first.
    # Splitting batches would allow the first control to reset before the workers.
    inp = {'project': 'default', 'owner_uid': config['owner_uid'], 'hosts': source['hosts'], 'inventory': source['inventory'],
           'parameters': {}, 'download_parallel': 1, 'batch_size': len(config['nodes']), 'playbook': playbook}
    path = output / 'plans' / (name + '.input.json')
    render.write(path, inp)
    path.with_suffix('.yaml').write_text(yaml.safe_dump(inp, sort_keys=False))
    plan = json.loads(subprocess.check_output([str(Path(binary).resolve()), '-o', 'json', 'compile', 'plans', '--file', str(path)], text=True))
    render.write(output / 'plans' / (name + '.json'), plan)
    uid = 'kube-uninstall-' + render.sha(config['owner_uid'] + '/' + config['cluster_uid'] + '/' + ca +
                                        ('/resume/' + resume_from if resume_from else ''))[:32]
    render.write(output / 'workflows' / (name + '.json'), {'uid': uid,
        'name': '卸载 Kubernetes（保留 NFS 数据与镜像缓存）', 'playbook': playbook,
        'inventory': source['inventory'], 'targets': {'default': {'node_group_uid': config['node_group_uid'], 'hosts': 'all', 'owner_uid': config['owner_uid']}},
        'fixed': {}, 'parameters': {}, 'definitions': {digest: body}, 'download_parallel': 1,
        'batch_size': len(config['nodes']), 'provenance': {'example': 'community-kubernetes-uninstall', 'ca_sha256': ca}})
    directory = output / 'workflow'
    directory.mkdir()
    (directory / 'uninstall.sh').write_text(body)
    native = {'uid': uid, 'name': 'Remove this Kubernetes installation',
        'inventory': 'inventory.yaml', 'targets': {'default': {
            'node_group_uid': config['node_group_uid'], 'hosts': 'all', 'owner_uid': config['owner_uid']}},
        'batch_size': len(config['nodes']), 'playbook': {'api_version': 'converyn/v1', 'name': name, 'steps': []}}
    for step in steps:
        step = copy.deepcopy(step)
        step['operation'].pop('script_sha256')
        step['operation']['script'] = 'uninstall.sh'
        native['playbook']['steps'].append(step)
    (directory / 'inventory.yaml').write_text(yaml.safe_dump(source['inventory'], sort_keys=False))
    (directory / 'workflow.yaml').write_text(yaml.safe_dump(native, sort_keys=False))
    render.write(output / 'sequence.json', [{'stage': name, 'workflow_uid': uid,
        'request_id': 'uninstall-' + plan['sha256'][:32], 'preview_sha256': plan['sha256'], 'work': len(plan['work'])}])
    print('Compiled uninstall workflow (not registered or executed):', output)
    return plan


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--installation', type=Path, required=True, help='original generated installation directory with private-kubeadm')
    p.add_argument('--converyn', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--resume-from', help='continue a previously backed-up uninstall from this original Node UID, preserving earlier evidence')
    a = p.parse_args()
    render_uninstall(a.installation, a.converyn, a.output, a.resume_from)
