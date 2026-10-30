#!/usr/bin/env python3
"""Compile the community installation into ordinary Converyn host deliveries."""
import argparse
import copy
import hashlib
import ipaddress
import json
from pathlib import Path
import re
import shutil
import subprocess

import yaml

ROOT = Path(__file__).resolve().parents[3] / "example/kubernetes"
BASE = '/opt/converyn-example'
# These scalar settings cannot redirect cluster identity, credentials, network
# ranges or install arbitrary manifest resources through a run override.
EDITABLE_CONFIGURATIONS = {
    BASE + '/values/csi-nfs.json': ['/controller/replicas'],
    BASE + '/values/traefik.json': ['/ports/web/nodePort'],
    BASE + '/values/harbor.json': [
        '/persistence/persistentVolumeClaim/registry/size',
        '/persistence/persistentVolumeClaim/jobservice/jobLog/size',
        '/persistence/persistentVolumeClaim/database/size',
        '/persistence/persistentVolumeClaim/redis/size'],
    BASE + '/values/monitoring.json': [
        '/grafana/persistence/size', '/grafana/resources/requests/cpu', '/grafana/resources/requests/memory',
        '/prometheus/prometheusSpec/retention', '/prometheus/prometheusSpec/resources/requests/cpu',
        '/prometheus/prometheusSpec/resources/requests/memory',
        '/prometheus/prometheusSpec/storageSpec/volumeClaimTemplate/spec/resources/requests/storage',
        '/alertmanager/alertmanagerSpec/storage/volumeClaimTemplate/spec/resources/requests/storage'],
}


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def validate(c):
    expected = {'project', 'cluster_uid', 'node_group_uid', 'owner_uid', 'endpoint', 'pod_cidr',
                'service_cidr', 'nodes', 'nfs', 'harbor_host', 'harbor_https_nodeport',
                'harbor_admin_password_file', 'grafana_admin_password_file', 'verification_image'}
    if set(c) != expected:
        raise ValueError('unknown or missing cluster fields: ' + str(set(c) ^ expected))
    if set(c['nfs']) != {'server', 'share', 'storage_class'}:
        raise ValueError('unknown or missing NFS fields')
    for key in ['project', 'cluster_uid', 'node_group_uid', 'owner_uid']:
        if not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}', c[key]):
            raise ValueError('invalid identifier: ' + key)
    nodes = c['nodes']
    controls = sum(n['control_plane'] is True for n in nodes)
    if not 2 <= len(nodes) <= 20 or controls not in (1, 3) or controls == len(nodes):
        raise ValueError('example requires one or three control planes and at least one worker (2..20 nodes)')
    for field in ['uid', 'hostname', 'address']:
        if len({n[field] for n in nodes}) != len(nodes):
            raise ValueError('duplicate node ' + field)
    pod, service = [ipaddress.IPv4Network(c[k]) for k in ['pod_cidr', 'service_cidr']]
    if pod.overlaps(service):
        raise ValueError('Pod and Service networks overlap')
    for node in nodes:
        if set(node) != {'uid', 'hostname', 'address', 'control_plane'} or type(node['control_plane']) is not bool:
            raise ValueError('invalid node fields or control_plane flag')
        if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,62}', node['hostname']):
            raise ValueError('invalid hostname')
        if not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_.-]{0,63}', node['uid']):
            raise ValueError('invalid node UID')
        address = ipaddress.IPv4Address(node['address'])
        if address in pod or address in service:
            raise ValueError('node address overlaps cluster networks')
    host, port = c['endpoint'].rsplit(':', 1)
    if not re.fullmatch(r'[a-zA-Z0-9.-]+', host) or not 1 <= int(port) <= 65535:
        raise ValueError('endpoint must be a host:port')
    if controls == 3 and host in {n['address'] for n in nodes}:
        raise ValueError('HA endpoint must be an independently managed load balancer or DNS name')
    ipaddress.IPv4Address(c['nfs']['server'])
    if not re.fullmatch(r'/[a-zA-Z0-9_./-]+', c['nfs']['share']) or '..' in c['nfs']['share'].split('/'):
        raise ValueError('invalid NFS export')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]*', c['nfs']['storage_class']):
        raise ValueError('invalid StorageClass')
    if not re.fullmatch(r'[a-z0-9][a-z0-9.-]+[a-z0-9]', c['harbor_host']):
        raise ValueError('invalid Harbor DNS name')
    if not 30000 <= c['harbor_https_nodeport'] <= 32767:
        raise ValueError('invalid HTTPS NodePort')
    for key in ['harbor_admin_password_file', 'grafana_admin_password_file']:
        if not re.fullmatch(r'/etc/converyn-example/[a-z0-9-]+', c[key]):
            raise ValueError('private password files must be in /etc/converyn-example')
    if not re.fullmatch(r'[a-zA-Z0-9./_:@-]+', c['verification_image']) or ':latest' in c['verification_image']:
        raise ValueError('verification image must use a fixed tag or digest')
    return next(n for n in nodes if n['control_plane'])


def load_workflow():
    recipe = yaml.safe_load((ROOT / 'stages.yaml').read_text())
    stages = recipe['stages']
    names = [v['id'] for v in stages]
    expected = ['00-preflight', '01-runtime', '02-control-plane', '02b-secondary-control-planes',
                '03-workers', '04-calico', '05-storage', '06-cert-manager', '07-ingress',
                '08-harbor', '09-monitoring', '10-verify']
    if names != expected:
        raise ValueError('workflow must preserve the complete ordered deployment stages')
    if stages[3]['batch_size'] != 1:
        raise ValueError('secondary stacked-etcd members must join serially')
    for stage in stages:
        if 'script' in stage and not re.fullmatch(r'[a-z][a-z0-9-]*', stage['script']):
            raise ValueError('workflow scripts must name a local script')
        if stage['kind'] == 'addon' and stage['targets'] != 'first-control':
            raise ValueError('addon stages must use the first control node')
    return recipe


def public_files(c, recipe=None):
    recipe = recipe or load_workflow()
    bindings = {'POD_CIDR': c['pod_cidr'], 'NFS_SERVER': c['nfs']['server'],
                'NFS_SHARE': c['nfs']['share'], 'STORAGE_CLASS': c['nfs']['storage_class'],
                'HARBOR_HOST': c['harbor_host'], 'HTTPS_NODEPORT': c['harbor_https_nodeport']}

    def resolve(value):
        if isinstance(value, dict):
            return {key: resolve(v) for key, v in value.items()}
        if isinstance(value, list):
            return [resolve(v) for v in value]
        if isinstance(value, str):
            for key, replacement in bindings.items():
                marker = '${' + key + '}'
                # Whole-value substitutions retain numbers such as NodePort.
                if value == marker:
                    return replacement
                value = value.replace(marker, str(replacement))
        return value

    files = {}
    for item in recipe['files']:
        source = (ROOT / item['source']).resolve()
        if not source.is_relative_to((ROOT / 'configs').resolve()):
            raise ValueError('configuration templates must stay inside configs/')
        value = source.read_text()
        if item['format'] == 'yaml':
            value = json.dumps(resolve(yaml.safe_load(value)), indent=2) + '\n'
        else:
            value = resolve(value)
        files[item['target']] = value
    return files


def native_directory(output, config, workflow, groups, node_group_snapshot=None, blob_root=None):
    """Expose the installation as an ordinary, editable native workflow directory.

    The compiler exports remain useful for inspection/removal tooling;
    users register this directory instead of assembling their own workflow JSON.
    Private kubeadm materials remain outside this registration root.
    """
    directory = output / 'workflow'
    sources = directory / 'prepared'
    sources.mkdir(parents=True)
    manifest = yaml.safe_load((ROOT / 'workflow.yaml').read_text())
    manifest['uid'] = workflow['uid']
    manifest['name'] = workflow['name']
    manifest['targets']['default']['owner_uid'] = config['owner_uid']
    nodes = [n['uid'] for n in config['nodes']]
    inventory = copy.deepcopy(workflow['inventory'])
    (directory / 'inventory.yaml').write_text(yaml.safe_dump(inventory, sort_keys=False))
    manifest['targets'] = copy.deepcopy(workflow['targets'])
    steps = {s['id']: s for s in workflow['playbook']['steps']}
    stages, renamed = {}, {}
    for group in groups:
        entries = []
        for index, step_id in enumerate(group['steps']):
            step = steps[step_id]
            selector = step['hosts']
            selected = nodes if selector == 'all' else workflow['inventory']['groups'][selector]
            if selected == nodes:
                hosts = 'all'
            elif len(selected) == 1:
                hosts = 'all[' + str(nodes.index(selected[0])) + ']'
            else:
                raise ValueError('native directory requires all-node or single-node stages')
            operation = copy.deepcopy(step['operation'])
            if operation['kind'] == 'binary' and blob_root is not None:
                artifact = operation['artifact']
                blob = Path(blob_root) / artifact['sha256']
                if blob.is_symlink() or not blob.is_file() or blob.stat().st_size != artifact['size']:
                    raise ValueError('missing or unsafe prepared artifact: ' + artifact['sha256'])
                checksum = hashlib.sha256()
                with blob.open('rb') as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                        checksum.update(chunk)
                if checksum.hexdigest() != artifact['sha256']:
                    raise ValueError('prepared artifact digest mismatch')
                destination = directory / 'assets' / artifact['sha256']
                destination.parent.mkdir(exist_ok=True)
                if not destination.exists():
                    shutil.copyfile(blob, destination)
                operation['artifact'] = {'$artifact': 'assets/' + artifact['sha256']}
            field = {'exec': 'script_sha256', 'file': 'sha256'}.get(operation['kind'])
            if field:
                text = workflow['definitions'][operation.pop(field)]
                relative = 'prepared/' + step_id + ('.sh' if operation['kind'] == 'exec' else '.txt')
                (directory / relative).write_text(text)
                operation['script' if operation['kind'] == 'exec' else 'source'] = relative
            selection = {'hosts': hosts}
            entries.append(dict(selection, operation=operation))
            renamed[step_id] = group['id'] + '-' + str(index)
        stages[group['id']] = entries
    # The standalone topology has no secondary control planes.
    stages.setdefault('02b-secondary-control-planes', [])
    manifest['parameters'] = copy.deepcopy(workflow['parameters'])
    for parameter in manifest['parameters'].values():
        for binding in parameter['bindings']:
            binding['step_id'] = renamed[binding['step_id']]
    manifest['provenance'] = {'example': 'community-kubernetes', 'step_groups': json.dumps([
        {'id': g['id'], 'steps': [renamed[s] for s in g['steps']]} for g in groups], separators=(',', ':'))}
    (directory / 'artifacts.yaml').write_text(yaml.safe_dump({'stages': stages}, sort_keys=False))
    (directory / 'workflow.yaml').write_text(yaml.safe_dump(manifest, sort_keys=False, allow_unicode=True))


def render(c, assets, binary, output, kubeadm_kit=None, node_group_snapshot=None, blob_root=None):
    control = validate(c)
    ordered_uids = [n['uid'] for n in c['nodes']]
    if node_group_snapshot is None:
        group = {'uid': c['node_group_uid'], 'name': c['node_group_uid'], 'revision': 1, 'node_uids': ordered_uids}
    else:
        if not isinstance(node_group_snapshot, dict) or set(node_group_snapshot) != {'uid', 'name', 'revision', 'node_uids'}:
            raise ValueError('node group snapshot must contain uid, name, revision and node_uids')
        if node_group_snapshot['uid'] != c['node_group_uid'] or type(node_group_snapshot['revision']) is not int or node_group_snapshot['revision'] < 1:
            raise ValueError('node group identity or revision mismatch')
        if not set(ordered_uids) <= set(node_group_snapshot['node_uids']):
            raise ValueError('inventory nodes outside node group scope')
        group = copy.deepcopy(node_group_snapshot)
    hosts = {'node_group_uid': group['uid'], 'revision': group['revision'], 'node_uids': ordered_uids}
    inventory = {'nodes': [{k:n[k] for k in ('uid','address','hostname')} for n in c['nodes']], 'groups': {}}
    recipe = load_workflow()
    definitions_by_stage = {v['id']: v for v in recipe['stages']}
    if output.exists():
        raise ValueError('output already exists; preserve previous plans and private kit')
    output.mkdir(parents=True, mode=0o700)
    kit = output / 'private-kubeadm'
    request = output / 'prepare-kubeadm.json'
    write(request, {'version': 'v1.36.4', 'endpoint': c['endpoint'], 'advertise_address': control['address'],
                    'pod_cidr': c['pod_cidr'], 'service_cidr': c['service_cidr']})
    if kubeadm_kit:
        previous = json.loads((kubeadm_kit / 'manifest.json').read_text())
        if previous['endpoint'] != c['endpoint'] or previous['version'] != 'v1.36.4':
            raise ValueError('existing kubeadm kit belongs to a different cluster endpoint/version')
        for name, digest in previous['config_sha256'].items():
            if sha((kubeadm_kit / name).read_text()) != digest:
                raise ValueError('existing kubeadm configuration digest mismatch')
        # The embedded package emits plain scalars for these validated IPv4/CIDR
        # fields. Reusing a kit must not pair its old network with a new CNI pool.
        initial = (kubeadm_kit / 'init.yaml').read_text()
        for key, value in [('podSubnet', c['pod_cidr']), ('serviceSubnet', c['service_cidr']),
                           ('advertiseAddress', control['address'])]:
            if not re.search(r'^\s*' + key + r': ' + re.escape(value) + r'\s*$', initial, re.M):
                raise ValueError('existing kubeadm kit differs at ' + key)
        if hashlib.sha256((kubeadm_kit / 'ca.crt').read_bytes()).hexdigest() != previous['ca_sha256']:
            raise ValueError('existing kubeadm CA digest mismatch')
        shutil.copytree(kubeadm_kit, kit)
    else:
        subprocess.run([str(binary), 'prepare', 'plugins', 'converyn.kubeadm', '--action', 'install',
                        '--input', str(request), '--output', str(kit)], check=True, capture_output=True)
    km = json.loads((kit / 'manifest.json').read_text())
    write(output / 'node-group.json', {'action': 'create_node_group' if group['revision'] == 1 else 'update_node_group', 'expected_revision': group['revision'] - 1, 'group': group})
    definitions = output / 'definitions'
    definitions.mkdir()
    stages = []
    addon_inputs = {v['script']: v['inputs'] for v in recipe['stages'] if v['kind'] == 'addon'}

    def definition(content):
        digest = sha(content)
        (definitions / (digest + '.txt')).write_text(content)
        return digest

    def step(name, op, nodes):
        return {'id': name, 'phase': 'rollout', 'scope': 'per_node',
                'node_uids': nodes, 'operation': op}

    def script(name, nodes, environment=None, addons=False, timeout=1800):
        environment = dict(environment or {})
        body = ((ROOT / 'scripts/common.sh').read_text() + '\n' if addons else '') + (ROOT / 'scripts' / (name + '.sh')).read_text()
        if addons:
            # Each registered addon stage binds the bytes it reads,
            # rather than assuming that a path still contains an earlier stage's file.
            selected = {BASE + '/' + relative for relative in addon_inputs[name]}
            inputs = {v['target']: v['sha256'] for v in assets['artifacts']
                      if v['target'] in selected or v['target'] in ['/usr/local/bin/helm', '/usr/local/bin/kubectl']}
            inputs.update({path: sha(content) for path, content in paths.items() if path in selected})
            checks = ''.join(digest + '  ' + path + '\n' for path, digest in sorted(inputs.items()) if path not in editable_paths)
            configuration_checks = ''
            for path, digest in sorted(inputs.items()):
                if path in editable_paths:
                    key = configuration_digest_key(path)
                    environment[key] = digest
                    configuration_checks += 'printf \'%s  %s\\n\' "$' + key + '" \'' + path + '\' | sha256sum --check --status\n'
            body = "set -euo pipefail\nsha256sum --check --status <<'CONVERYN_FIXED_INPUTS'\n" + checks + 'CONVERYN_FIXED_INPUTS\n' + configuration_checks + body
        op = {'kind': 'exec', 'script_sha256': definition(body), 'resources': ['kubernetes-host'],
              'execution': 'host', 'environment': environment or {}, 'timeout_seconds': timeout}
        return step(name, op, nodes)

    all_nodes = [n['uid'] for n in c['nodes']]
    cp = [control['uid']]
    preflight = []
    for node in c['nodes']:
        check = script(definitions_by_stage['00-preflight']['script'], [node['uid']], {'NODE_NAME': node['hostname']})
        check['id'] += '-' + node['uid']
        preflight.append(check)
    stages.append(('00-preflight', preflight))
    install = []
    paths = public_files(c, recipe)
    editable_paths = set(EDITABLE_CONFIGURATIONS) & set(paths)
    def configuration_digest_key(path):
        return 'CONFIG_' + sha(path)[:16].upper() + '_SHA256'
    for i, item in enumerate(assets['artifacts']):
        nodes = all_nodes if item['target'].startswith(('/usr/local/', '/opt/cni/')) or item['target'] == BASE + '/images.tar' else cp
        op = {'kind': 'binary', 'path': item['target'], 'mode': item['mode'], 'artifact': {
            'sha256': item['sha256'], 'size': item['size']}}
        install.append(step('asset-' + str(i), op, nodes))
    for i, (path, content) in enumerate(paths.items()):
        nodes = cp if path.startswith(BASE) else all_nodes
        install.append(step('config-' + str(i), {'kind': 'file', 'path': path, 'sha256': definition(content), 'mode': 0o644}, nodes))
    archive = next((v for v in assets['artifacts'] if v['target'] == BASE + '/images.tar'), None)
    install.append(script(definitions_by_stage['01-runtime']['script'], all_nodes,
                          {'IMAGE_ARCHIVE_SHA256': archive['sha256']} if archive else {}))
    stages.append(('01-runtime', install))
    choices = {'first-control': [control],
               'secondary-controls': [n for n in c['nodes'] if n['control_plane'] and n != control],
               'workers': [n for n in c['nodes'] if not n['control_plane']]}
    for stage in [v for v in recipe['stages'] if v['kind'] == 'kubeadm']:
        name, chosen = stage['id'], choices[stage['targets']]
        if not chosen:
            continue
        steps = []
        for node in chosen:
            action, config = stage['action'], stage['config']
            env = {'CA_FILE': '/etc/converyn-example/kubeadm/ca.crt', 'CA_SHA256': km['ca_sha256'],
                   'NODE_NAME': node['hostname'], 'KUBECONFIG': '/etc/kubernetes/admin.conf' if node['control_plane'] else '/etc/kubernetes/kubelet.conf',
                   'BINARY_SHA256': next(v['sha256'] for v in assets['artifacts'] if v['target'] == '/usr/local/bin/kubeadm'),
                   'CONFIG_PATH': '/etc/converyn-example/kubeadm/' + config, 'CONFIG_SHA256': km['config_sha256'][config]}
            inp = output / (node['uid'] + '-expand.json')
            write(inp, {'version': 'v1.36.4', 'environment': env, 'execution': 'host', 'timeout_seconds': 1200})
            expansion = output / ('expanded-' + node['uid'])
            subprocess.run([str(binary), 'expand', 'plugins', 'converyn.kubeadm', '--action', action,
                            '--input', str(inp), '--output', str(expansion)], check=True, capture_output=True)
            for path in (expansion / 'definitions').iterdir():
                definition(path.read_text())
            op = json.loads((expansion / 'playbook.json').read_text())['steps'][0]['operation']
            steps.append(step(action + '-' + node['uid'], op, [node['uid']]))
        stages.append((name, steps))
    bindings = dict(c, node_count=len(c['nodes']), control_count=sum(n['control_plane'] for n in c['nodes']),
                    control_address=control['address'], storage_class=c['nfs']['storage_class'])
    for stage in [v for v in recipe['stages'] if v['kind'] == 'addon']:
        name, action = stage['id'], stage['script']
        env = {key: str(bindings[field]) for key, field in stage['environment'].items()}
        selected = {BASE + '/' + relative for relative in addon_inputs[action]}
        inputs = []
        for i, item in enumerate(assets['artifacts']):
            if item['target'] in selected:
                inputs.append(step('input-asset-' + str(i), {'kind': 'binary', 'path': item['target'], 'mode': item['mode'],
                    'artifact': {'sha256': item['sha256'], 'size': item['size']}}, cp))
        for i, (path, content) in enumerate(paths.items()):
            if path in selected:
                inputs.append(step('input-config-' + str(i), {'kind': 'file', 'path': path,
                    'sha256': definition(content), 'mode': 0o644}, cp))
        inputs.append(script(action, cp, env, addons=True, timeout=stage['timeout_seconds']))
        stages.append((name, inputs))
    order = {v['id']: i for i, v in enumerate(recipe['stages'])}
    stages.sort(key=lambda v: order[v[0]])
    # One full inventory batch gives every Step a global success barrier. In
    # particular runtime must finish on every node before kubeadm init starts.
    # Each secondary control plane already has its own single-node Step.
    combined = []
    groups = []
    for name, steps in stages:
        if definitions_by_stage[name]['kind'] == 'addon':
            # These exact inputs were delivered by runtime in the same Run.
            steps = [s for s in steps if not s['id'].startswith('input-')]
        ids = []
        for item in steps:
            item = copy.deepcopy(item)
            item['id'] = name + '--' + item['id']
            ids.append(item['id'])
            combined.append(item)
        groups.append({'id': name, 'steps': ids})
    parameters = {}
    for item in combined:
        op = item['operation']
        if op['kind'] != 'file' or op['path'] not in editable_paths:
            continue
        path = op['path']
        key = re.sub('[^A-Z0-9_]', '_', path.removeprefix(BASE + '/').upper())
        rules = {pointer: {'kind': 'duration' if pointer.endswith('/retention') else 'quantity'}
                 for pointer in EDITABLE_CONFIGURATIONS[path]}
        if path.endswith('/csi-nfs.json'):
            rules['/controller/replicas'] = {'kind': 'integer', 'minimum': 1, 'maximum': 10}
        if path.endswith('/traefik.json'):
            rules['/ports/web/nodePort'] = {'kind': 'integer', 'minimum': 30000, 'maximum': 32767}
        bindings = [{'step_id': item['id'], 'path': '/content', 'allowed_json_paths': EDITABLE_CONFIGURATIONS[path], 'scalar_rules': rules}]
        digest_key = configuration_digest_key(path)
        bindings += [{'step_id': s['id'], 'path': '/environment/' + digest_key, 'transform': 'sha256'}
                     for s in combined if digest_key in s['operation'].get('environment', {})]
        parameters[key] = {'kind': 'string', 'default': paths[path], 'max_length': 65536, 'bindings': bindings}
    stages = [('00-install-kubernetes', combined)]
    sequence = []
    for name, steps in stages:
        for step in steps:
            selected = step.pop('node_uids')
            selector = 'all' if selected == ordered_uids else 'step-' + step['id']
            if selector != 'all': inventory['groups'][selector] = selected
            step['hosts'] = selector
        playbook = {'api_version': 'converyn/v1', 'name': name, 'steps': steps}
        # Join stacked-etcd members one at a time; concurrent membership changes
        # can strand an otherwise healthy initial control plane.
        batch_size = len(all_nodes)
        inp = {'project': 'default', 'owner_uid': c['owner_uid'], 'hosts': hosts, 'inventory': inventory,
               'parameters': {}, 'download_parallel': 4, 'batch_size': batch_size, 'playbook': playbook}
        path = output / 'plans' / (name + '.input.json')
        write(path, inp)
        path.with_suffix('.yaml').write_text(yaml.safe_dump(inp, sort_keys=False))
        playbook_path = output / 'playbooks' / (name + '.yaml')
        playbook_path.parent.mkdir(exist_ok=True)
        playbook_path.write_text(yaml.safe_dump(playbook, sort_keys=False))
        result = subprocess.run([str(binary), '-o', 'json', 'compile', 'plans', '--file', str(path)], check=True, capture_output=True, text=True)
        plan = json.loads(result.stdout)
        write(output / 'plans' / (name + '.json'), plan)
        workflow_uid = 'kube-' + sha(c['owner_uid'] + '/' + name)[:32]
        used = {step['operation'].get('script_sha256') if step['operation']['kind']=='exec' else step['operation'].get('sha256') for step in steps if step['operation']['kind'] in ['exec','file']}
        fixed_definitions = {digest:(definitions/(digest+'.txt')).read_text() for digest in used}
        workflow = {
            'uid': workflow_uid, 'name': '部署 Kubernetes 与基础组件', 'playbook': playbook,
            'inventory': inventory, 'targets': {'default': {'node_group_uid': c['node_group_uid'], 'hosts': 'all', 'owner_uid': c['owner_uid']}},
            'fixed': {}, 'parameters': parameters, 'definitions': fixed_definitions, 'download_parallel': 4,
            'batch_size': batch_size, 'provenance': {'example':'community-kubernetes','step_groups':json.dumps(groups, separators=(',', ':'))}}
        write(output / 'workflows' / (name + '.json'), workflow)
        native_directory(output, c, workflow, groups, node_group_snapshot, blob_root=blob_root)
        subprocess.run([str(binary), 'validate', 'workflows', '--file', str(output / 'workflow')],
                       check=True, capture_output=True)
        sequence.append({'stage':name,'workflow_uid':workflow_uid,'request_id':'stage-'+plan['sha256'][:32], 'preview_sha256':plan['sha256'],'work':len(plan['work'])})
    write(output / 'sequence.json', sequence)
    (output / 'sequence.yaml').write_text(yaml.safe_dump(sequence, sort_keys=False))
    write(output / 'public-cluster.json', c)
    print('Compiled', len(sequence), 'stages:', output)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--config', type=Path, required=True)
    p.add_argument('--assets', type=Path, required=True)
    p.add_argument('--converyn', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--kubeadm-kit', type=Path, help='reuse the original private kit when reviewing an addon repair')
    p.add_argument('--node-group-snapshot', type=Path, help='JSON file containing the registered NodeGroup (not an action wrapper)')
    a = p.parse_args()
    snapshot = json.loads(a.node_group_snapshot.read_text()) if a.node_group_snapshot else None
    render(yaml.safe_load(a.config.read_text()), json.loads(a.assets.read_text()), a.converyn.resolve(), a.output.resolve(), a.kubeadm_kit, snapshot, a.assets.resolve().parent / 'blobs')
