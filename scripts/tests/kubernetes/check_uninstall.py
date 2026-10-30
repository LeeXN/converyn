#!/usr/bin/env python3
"""Compile removal workflows and check ordering/identity without executing scripts."""
from _paths import EXAMPLE, TOOLS
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

import render
import render_uninstall

PARSER = argparse.ArgumentParser(description=__doc__)
PARSER.add_argument('--converyn', type=Path, required=True)
ARGS = PARSER.parse_args()


class UninstallExample(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='converyn-uninstall-check-')
        cls.base = Path(cls.temp.name)
        cls.installation = cls.base / 'installation'
        cls.config = yaml.safe_load((render.ROOT / 'cluster-ha.yaml').read_text())
        assets = {'artifacts': [{'target': '/usr/local/bin/kubeadm', 'mode': 493, 'sha256': 'a' * 64, 'size': 1}]}
        render.render(cls.config, assets, ARGS.converyn.resolve(), cls.installation)
        cls.output = cls.base / 'uninstall'
        cls.plan = render_uninstall.render_uninstall(cls.installation, ARGS.converyn, cls.output)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_actual_compiler_orders_all_backups_before_workers_then_controls(self):
        work = self.plan['work']
        nodes = self.config['nodes']
        control = next(n for n in nodes if n['control_plane'])
        expected = ([n['uid'] for n in nodes if not n['control_plane']] +
                    [n['uid'] for n in nodes if n['control_plane'] and n != control] + [control['uid']])
        reset = [w for w in work if w['operation']['environment']['UNINSTALL_ACTION'] == 'reset']
        self.assertEqual([w['node_uid'] for w in reset], expected)
        self.assertEqual(len({w['wave'] for w in reset}), len(reset))
        self.assertTrue(all(w['phase'] == 'prepare' for w in work[:2 * len(nodes)]))
        self.assertTrue(all(w['phase'] == 'rollout' for w in work[2 * len(nodes):]))
        self.assertEqual({w['batch'] for w in reset}, {0})

    def test_native_yaml_compiles_and_registered_workflow_is_immutable(self):
        path = self.output / 'plans/90-uninstall-kubernetes.input.yaml'
        compiled = json.loads(subprocess.check_output([str(ARGS.converyn.resolve()), '-o', 'json', 'compile', 'plans', '--file', str(path)], text=True))
        self.assertEqual(compiled['sha256'], self.plan['sha256'])
        workflow = json.loads((self.output / 'workflows/90-uninstall-kubernetes.json').read_text())
        native = subprocess.run([str(ARGS.converyn.resolve()), 'validate', 'workflows', '--file', str(self.output / 'workflow')], capture_output=True, text=True)
        self.assertEqual(native.returncode, 0, native.stderr)
        self.assertEqual(workflow['parameters'], {})
        self.assertEqual(workflow['targets']['default']['owner_uid'], self.config['owner_uid'])
        for digest, body in workflow['definitions'].items():
            self.assertEqual(hashlib.sha256(body.encode()).hexdigest(), digest)
            self.assertNotIn('PRIVATE KEY-----', body)
        self.assertEqual(self.output.stat().st_mode & 0o077, 0)
        for work in self.plan['work']:
            self.assertEqual(work['operation']['execution'], 'host')

    def test_tampered_ca_and_endpoint_rejected_and_previous_output_preserved(self):
        kit = self.installation / 'private-kubeadm/manifest.json'
        original = kit.read_text()
        for field, value in [('ca_sha256', 'b' * 64), ('endpoint', 'other.example.test:6443')]:
            manifest = json.loads(original)
            manifest[field] = value
            kit.write_text(json.dumps(manifest))
            try:
                with self.assertRaisesRegex(ValueError, 'CA/endpoint'):
                    render_uninstall.render_uninstall(self.installation, ARGS.converyn, self.base / ('bad-' + field))
            finally:
                kit.write_text(original)
        with self.assertRaisesRegex(ValueError, 'output already exists'):
            render_uninstall.render_uninstall(self.installation, ARGS.converyn, self.output)

    def test_script_rejects_wrong_host_before_any_destructive_command(self):
        env = dict(os.environ, UNINSTALL_ACTION='reset', CA_SHA256='a' * 64,
            BINARY_SHA256='b' * 64, CLUSTER_ENDPOINT='unused.example.test:6443',
            NODE_NAME='invalid-uninstall-host-never-matches', NODE_ADDRESS='192.0.2.254',
            NODE_UID='uninstall-test', FIRST_CONTROL='false', KUBECONFIG='/nonexistent/uninstall-check')
        result = subprocess.run(['bash', str(render.ROOT / 'scripts/uninstall.sh')], env=env,
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('host identity mismatch', result.stderr)
        self.assertEqual(result.stdout, '')

    def test_resume_keeps_prior_backups_and_continues_only_remaining_nodes(self):
        node = [n for n in self.config['nodes'] if n['control_plane']][2]['uid']
        out = self.base / 'resume'
        resumed = render_uninstall.render_uninstall(self.installation, ARGS.converyn, out, node)
        work = resumed['work']
        self.assertEqual(len(work), 2)
        self.assertEqual([w['node_uid'] for w in work], [node, self.config['nodes'][0]['uid']])
        self.assertEqual([w['operation']['environment']['UNINSTALL_ACTION'] for w in work], ['resume', 'resume'])
        self.assertEqual([w['operation']['environment']['EXPECTED_ETCD_MEMBERS'] for w in work], ['1', '0'])
        invalid = self.base / 'unknown-resume'
        with self.assertRaisesRegex(ValueError, 'outside the original'):
            render_uninstall.render_uninstall(self.installation, ARGS.converyn, invalid, 'not-an-original-node')
        self.assertFalse(invalid.exists())

    def test_generation_never_registers_or_executes_and_scripts_are_syntax_checked(self):
        source = (TOOLS / 'render_uninstall.py').read_text()
        self.assertNotIn("'run'", source)
        self.assertNotIn("'register'", source)
        script = render.ROOT / 'scripts/uninstall.sh'
        subprocess.run(['bash', '-n', str(script)], check=True)
        text = script.read_text()
        self.assertNotIn('rm -rf', text)
        self.assertNotIn('iptables -F', text)
        self.assertNotIn('helm uninstall', '\n'.join(l for l in text.splitlines() if not l.startswith('#')))
        self.assertIn('systemUUID', text)
        self.assertIn('cluster CA mismatch', text)
        self.assertIn('kubernetes-config.tgz', text)


if __name__ == '__main__':
    unittest.main(argv=['check_uninstall.py'])
