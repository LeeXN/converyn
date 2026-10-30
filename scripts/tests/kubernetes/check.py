#!/usr/bin/env python3
"""Validate real plan compilation, secret separation and fail-stop stage delivery."""
from _paths import EXAMPLE, TOOLS
import argparse
import ast
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import tarfile
import unittest

import yaml
from unittest.mock import patch
from unittest.mock import Mock
from types import SimpleNamespace

import render
import prepare

ROOT = EXAMPLE
PARSER = argparse.ArgumentParser(description=__doc__)
PARSER.add_argument('--converyn', type=Path, required=True)
PARSER.add_argument('--assets', type=Path, required=True)
PARSER.add_argument('--helm', type=Path, help='enable real Helm manifest checks with --chart-directory (requires PyYAML)')
PARSER.add_argument('--chart-directory', type=Path)
ARGS = PARSER.parse_args()


class CommunityExample(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='converyn-community-check-')
        cls.output = Path(cls.temp.name) / 'rendered'
        cls.config = yaml.safe_load((ROOT / 'cluster.yaml').read_text())
        render.render(cls.config, json.loads(ARGS.assets.read_text()), ARGS.converyn.resolve(), cls.output)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_actual_plans_are_ordered_and_scripts_are_in_registered_snapshots(self):
        sequence = json.loads((self.output / 'sequence.json').read_text())
        self.assertEqual(len(sequence), 1)
        self.assertEqual(len(list((self.output / 'workflows').glob('*.json'))), 1)
        for stage in sequence:
            plan = json.loads((self.output / 'plans' / (stage['stage'] + '.json')).read_text())
            self.assertEqual(stage['preview_sha256'], plan['sha256'])
            self.assertGreater(len(plan['work']), 0)
            for work in plan['work']:
                op = work['operation']
                if op['kind'] != 'exec':
                    continue
                self.assertEqual(op['execution'], 'host')
                workflow=json.loads((self.output/'workflows'/(stage['stage']+'.json')).read_text())
                self.assertIn(op['script_sha256'],workflow['definitions'])
                self.assertEqual(workflow['targets']['default']['owner_uid'],self.config['owner_uid'])

    def test_definitions_match_hash_and_do_not_contain_private_kubeadm_material(self):
        for path in (self.output / 'definitions').iterdir():
            body = path.read_bytes()
            self.assertEqual(hashlib.sha256(body).hexdigest(), path.stem)
            self.assertLessEqual(len(body), 65536)
            self.assertNotIn(b'PRIVATE KEY-----', body)
            self.assertNotIn(b'bootstrapTokens:', body)
        self.assertEqual(self.output.stat().st_mode & 0o077, 0)
        with self.assertRaisesRegex(ValueError, 'output already exists'):
            render.render(self.config, {}, ARGS.converyn.resolve(), self.output)

    def test_native_yaml_inputs_compile_to_the_same_plans(self):
        for stage in json.loads((self.output / 'sequence.json').read_text()):
            stem = self.output / 'plans' / (stage['stage'] + '.input')
            self.assertEqual(yaml.safe_load(stem.with_suffix('.input.yaml').read_text()),
                             json.loads(stem.with_suffix('.input.json').read_text()))
            compiled = json.loads(subprocess.check_output([str(ARGS.converyn.resolve()), '-o', 'json', 'compile', 'plans',
                                  '--file', str(stem.with_suffix('.input.yaml'))], text=True))
            self.assertEqual(compiled['sha256'], stage['preview_sha256'])

    def test_native_directory_preserves_operations_targets_and_parameter_bindings(self):
        directory = self.output / 'workflow'
        checked = json.loads(subprocess.check_output([
            str(ARGS.converyn.resolve()), '-o', 'json', 'validate', 'workflows', '--file', str(directory)], text=True))
        native = yaml.safe_load((directory / 'workflow.yaml').read_text())
        exported = json.loads((self.output / 'workflows/00-install-kubernetes.json').read_text())
        self.assertEqual(native['uid'], exported['uid'])
        self.assertNotIn('cluster_uid', native['targets']['default'])
        self.assertEqual(native['targets']['default']['owner_uid'], self.config['owner_uid'])
        actual = checked['requirements']['steps']
        self.assertEqual(len(actual), len(exported['playbook']['steps']))
        renamed = {}
        for old, new in zip(exported['playbook']['steps'], actual):
            renamed[old['id']] = new['step']
            self.assertEqual(new['nodes'], [n['uid'] for n in exported['inventory']['nodes']] if old['hosts']=='all' else exported['inventory']['groups'][old['hosts']])
            for key, value in old['operation'].items():
                self.assertEqual(new['operation'].get(key, {} if value == {} else None), value)
        for name, parameter in native['parameters'].items():
            expected = copy.deepcopy(exported['parameters'][name])
            for binding in expected['bindings']:
                binding['step_id'] = renamed[binding['step_id']]
            self.assertEqual(parameter, expected)
        for path in directory.rglob('*'):
            if path.is_file():
                self.assertNotIn(b'PRIVATE KEY-----', path.read_bytes())
                self.assertNotIn(b'bootstrapTokens:', path.read_bytes())
        self.assertFalse((directory / 'private-kubeadm').exists())

    def test_reviewable_templates_preserve_typed_ports_and_host_variables(self):
        value = copy.deepcopy(self.config)
        value['harbor_https_nodeport'] = 31043
        value['nfs']['storage_class'] = 'manual-storage'
        files = render.public_files(value)
        ports = json.loads(files[render.BASE + '/values/traefik.json'])['ports']
        self.assertEqual(ports['websecure']['nodePort'], 31043)
        storage = json.loads(files[render.BASE + '/manifests/storage-class.json'])
        self.assertEqual(storage['metadata']['name'], 'manual-storage')
        self.assertIn('$KUBELET_KUBECONFIG_ARGS', files['/etc/systemd/system/kubelet.service'])

    def test_ha_proxy_preview_has_three_backends_and_rejects_duplicate_addresses(self):
        cmd = ['bash', str(ROOT / 'bootstrap-api-proxy.sh'), '--print', 'kube-ha.example.test']
        text = subprocess.check_output(cmd + ['192.0.2.10', '192.0.2.11', '192.0.2.12'], text=True)
        self.assertIn('bind 127.0.0.1:16443', text)
        self.assertEqual(text.count('  server cp'), 3)
        rejected = subprocess.run(cmd + ['192.0.2.10', '192.0.2.10', '192.0.2.12'], capture_output=True)
        self.assertNotEqual(rejected.returncode, 0)

    def test_input_rejects_overlap_unknown_secrets_and_unsafe_paths(self):
        for mutate in [lambda c: c.update(service_cidr=c['pod_cidr']),
                       lambda c: c.update(harbor_admin_password='do-not-publish'),
                       lambda c: c.update(harbor_admin_password_file='/etc/converyn/token'),
                       lambda c: c['nodes'][1].update(address=c['nodes'][0]['address'])]:
            value = copy.deepcopy(self.config)
            mutate(value)
            with self.assertRaises(ValueError):
                render.validate(value)

    def test_kit_reuse_rejects_an_unrelated_network(self):
        value = copy.deepcopy(self.config)
        value['pod_cidr'] = '10.247.0.0/16'
        with self.assertRaisesRegex(ValueError, 'kit differs at podSubnet'):
            render.render(value, {}, ARGS.converyn.resolve(), Path(self.temp.name) / 'wrong-kit',
                          self.output / 'private-kubeadm')

    def test_ha_plans_join_secondary_controls_serially_with_private_configs(self):
        value = copy.deepcopy(self.config)
        value['endpoint'] = 'kube-ha.example.test:16443'
        for i in range(4, 6):
            value['nodes'].append({'uid': 'kube' + str(i), 'hostname': 'kube' + str(i),
                                   'address': '192.0.2.' + str(9 + i), 'control_plane': True})
        output = Path(self.temp.name) / 'ha'
        render.render(value, json.loads(ARGS.assets.read_text()), ARGS.converyn.resolve(), output)
        sequence = json.loads((output / 'sequence.json').read_text())
        self.assertEqual(len(sequence), 1)
        km = json.loads((output / 'private-kubeadm/manifest.json').read_text())
        plan = json.loads((output / 'plans/00-install-kubernetes.json').read_text())
        secondary = [w for w in plan['work'] if w['step_id'].startswith('02b-secondary-control-planes--')]
        self.assertEqual({w['node_uid'] for w in secondary}, {'kube4', 'kube5'})
        self.assertEqual(len({w['wave'] for w in secondary}), 2)
        self.assertLess(secondary[0]['wave'], secondary[1]['wave'])
        runtime = [w for w in plan['work'] if w['step_id'] == '01-runtime--runtime']
        initial = [w for w in plan['work'] if w['step_id'].startswith('02-control-plane--')]
        workers = [w for w in plan['work'] if w['step_id'].startswith('03-workers--')]
        self.assertEqual({w['node_uid'] for w in runtime}, {n['uid'] for n in value['nodes']})
        self.assertLess(max(w['wave'] for w in runtime), min(w['wave'] for w in initial))
        self.assertLess(max(w['wave'] for w in initial), min(w['wave'] for w in secondary))
        self.assertLess(max(w['wave'] for w in secondary), min(w['wave'] for w in workers))
        for work in secondary:
            env = work['operation']['environment']
            self.assertTrue(env['CONFIG_PATH'].endswith('/join-control-plane.yaml'))
            self.assertEqual(env['CONFIG_SHA256'], km['config_sha256']['join-control-plane.yaml'])
            self.assertEqual(env['KUBECONFIG'], '/etc/kubernetes/admin.conf')
        for definition in (output / 'definitions').iterdir():
            self.assertNotIn(b'PRIVATE KEY-----', definition.read_bytes())
            self.assertNotIn(b'bootstrapTokens:', definition.read_bytes())
        delivery = json.loads((output / 'workflows/00-install-kubernetes.json').read_text())
        self.assertEqual(delivery['batch_size'], len(value['nodes']))
        self.assertLessEqual(len(delivery['playbook']['steps']), 64)
        verify = plan['work'][-1]['operation']['environment']
        self.assertEqual(verify['EXPECTED_CONTROL_PLANES'], '3')
        self.assertEqual(verify['EXPECTED_NODES'], '5')
        for mutate in [lambda c: c.update(endpoint=c['nodes'][0]['address'] + ':6443'),
                       lambda c: c['nodes'][-1].update(control_plane=False)]:
            invalid = copy.deepcopy(value)
            mutate(invalid)
            with self.assertRaises(ValueError):
                render.validate(invalid)

    def test_harbor_probe_preserves_tls_hostname_and_does_not_shadow_client(self):
        source = (ROOT / 'scripts/verify.sh').read_text().split("python3 - <<'PY'\n", 1)[1].rsplit('\nPY', 1)[0]
        function = next(v for v in ast.parse(source).body if isinstance(v, ast.FunctionDef) and v.name == 'http')
        connection = Mock()
        response = connection.getresponse.return_value
        response.status, response.getheaders.return_value, response.read.return_value = 200, [('Content-Type', 'text/plain')], b'ok'
        constructor, context, sockets = Mock(return_value=connection), Mock(), Mock()
        scope = {'HTTPSConnection': constructor, 'context': context, 'host': 'harbor.example.test', 'port': 30443,
                 'socket': sockets, 'os': SimpleNamespace(environ={'CONTROL_ADDRESS': '192.0.2.10'})}
        exec(compile(ast.Module(body=[function], type_ignores=[]), 'verify.sh', 'exec'), scope)
        self.assertEqual(scope['http']('GET', '/api/v2.0/ping'), (200, {'Content-Type': 'text/plain'}, b'ok'))
        constructor.assert_called_once_with('harbor.example.test', 30443, context=context, timeout=30)
        sockets.create_connection.assert_called_once_with(('192.0.2.10', 30443), 30)
        self.assertEqual(context.wrap_socket.call_args.kwargs['server_hostname'], 'harbor.example.test')
        connection.close.assert_called_once()

    def test_lock_only_uses_fixed_https_sources(self):
        lock = json.loads((ROOT / 'sources.lock.json').read_text())
        for item in lock['sources']:
            self.assertTrue(item['url'].startswith('https://'))
            self.assertNotIn('/latest/', item['url'])
            self.assertRegex(item['sha256'], r'^[a-f0-9]{64}$')

    def test_optional_image_archive_is_pinned_and_sent_to_every_node(self):
        base = Path(self.temp.name) / 'offline'
        base.mkdir()
        archive = base / 'images.tar'
        with tarfile.open(archive, 'w') as target:
            item = tarfile.TarInfo('manifest.json')
            item.size = 2
            target.addfile(item, io.BytesIO(b'[]'))
        (base / 'sources.lock.json').write_text(json.dumps({'sources': [], 'versions': {}}))
        with patch.object(prepare, 'ROOT', base):
            prepare.prepare(base / 'assets', archive)
        extra = json.loads((base / 'assets/artifacts.json').read_text())['artifacts'][0]
        self.assertEqual(extra['sha256'], hashlib.sha256(archive.read_bytes()).hexdigest())
        assets = json.loads(ARGS.assets.read_text())
        # The supplied bundle may already include a real offline archive.
        assets['artifacts'] = [v for v in assets['artifacts'] if v['target'] != extra['target']]
        assets['artifacts'].append(extra)
        output = base / 'rendered'
        render.render(self.config, assets, ARGS.converyn.resolve(), output, self.output / 'private-kubeadm')
        plan = json.loads((output / 'plans/00-install-kubernetes.json').read_text())
        targets = {w['node_uid'] for w in plan['work'] if w['operation'].get('path') == '/opt/converyn-example/images.tar'}
        self.assertEqual(targets, {n['uid'] for n in self.config['nodes']})
        for work in plan['work']:
            if work['step_id'] == '01-runtime--runtime':
                self.assertEqual(work['operation']['environment']['IMAGE_ARCHIVE_SHA256'], extra['sha256'])

    @unittest.skipUnless(ARGS.helm and ARGS.chart_directory, 'provide Helm and downloaded charts for rendered manifest checks')
    def test_rendered_charts_use_nodeport_persistence_and_existing_secrets(self):
        import yaml
        files = render.public_files(self.config)
        objects = {}
        for name, namespace in [('traefik', 'ingress-system'), ('harbor', 'harbor'), ('monitoring', 'monitoring')]:
            values = Path(self.temp.name) / (name + '-values.json')
            values.write_text(files['/opt/converyn-example/values/' + name + '.json'])
            output = subprocess.check_output([str(ARGS.helm.resolve()), 'template', name,
                str(ARGS.chart_directory.resolve() / (name + '.tgz')), '--namespace', namespace,
                '--values', str(values), '--kube-version', '1.36.4'], text=True)
            objects[name] = [v for v in yaml.safe_load_all(output) if v]
        service = next(v for v in objects['traefik'] if v['kind'] == 'Service' and v['metadata']['name'] == 'traefik')
        self.assertEqual(service['spec']['type'], 'NodePort')
        self.assertEqual(next(p['nodePort'] for p in service['spec']['ports'] if p['port'] == 443), self.config['harbor_https_nodeport'])
        ingress = next(v for v in objects['harbor'] if v['kind'] == 'Ingress')
        self.assertEqual(ingress['spec']['ingressClassName'], 'traefik')
        self.assertEqual(ingress['spec']['tls'][0]['secretName'], 'harbor-tls')
        core = next(v for v in objects['harbor'] if v['kind'] == 'Deployment' and v['metadata']['name'] == 'harbor-core')
        self.assertTrue(any(v.get('valueFrom', {}).get('secretKeyRef', {}).get('name') == 'harbor-admin'
                            for v in core['spec']['template']['spec']['containers'][0]['env']))
        prometheus = next(v for v in objects['monitoring'] if v['kind'] == 'Prometheus')
        self.assertEqual(prometheus['spec']['storage']['volumeClaimTemplate']['spec']['storageClassName'], self.config['nfs']['storage_class'])


if __name__ == '__main__':
    unittest.main(argv=[sys.argv[0]], verbosity=2)
