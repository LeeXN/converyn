"""Validate prepared package bytes and exercise the CI driver without a deployment."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]


class ReleaseExamples(unittest.TestCase):
    @unittest.skipUnless(shutil.which('dpkg-deb'), 'requires the operator package builder')
    def test_prepared_package_and_all_native_operations_compile(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'showcase'
            subprocess.run([os.sys.executable, str(ROOT / 'scripts/ops/showcase/prepare.py'), '--manager', 'deb', '--output', str(output)], check=True, capture_output=True)
            package = output / 'assets/converyn-showcase.deb'
            meta = subprocess.check_output(['dpkg-deb', '-f', str(package), 'Package', 'Version'], text=True)
            self.assertIn('converyn-showcase', meta)
            self.assertIn('1.0.0-1', meta)
            result = subprocess.run([str(ROOT / 'target/debug/converyn'), 'validate', 'workflows', '--file', str(output)], text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            workflow = yaml.safe_load((output / 'workflow.yaml').read_text())
            self.assertEqual({s['operation']['kind'] for s in workflow['playbook']['steps']}, {'plugin', 'package', 'binary', 'file', 'service', 'exec', 'http_check'})
            self.assertNotIn('postinst', subprocess.check_output(['dpkg-deb', '--ctrl-tarfile', str(package)]).decode('latin1'))

    def test_ci_driver_retries_same_release_without_changing_request_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / 'config.json'
            config.write_text('{}')
            config.chmod(0o600)
            tool = root / 'converyn'
            tool.write_text('#!/usr/bin/env python3\nimport json,os,sys\nwith open(os.environ["EVENTS"],"a") as output: output.write(json.dumps(sys.argv[1:])+"\\n")\n')
            tool.chmod(0o700)
            env = {**os.environ, 'PATH': str(root) + ':' + os.environ['PATH'], 'EVENTS': str(root / 'events'), 'CONVERYN_CONFIG': str(config), 'CONVERYN_WORKFLOW_DIR': 'example/node-demo', 'CONVERYN_WORKFLOW_UID': 'node-site-demo', 'CONVERYN_TARGET': 'default', 'DEPLOYMENT_ID': 'test-001', 'RELEASE_VERSION': 'v2'}
            for version in ['v2', 'v2', 'v1']:
                env['RELEASE_VERSION'] = version
                subprocess.run(['bash', str(ROOT / 'example/ci/deploy.sh')], env=env, check=True, capture_output=True)
            events = [json.loads(line) for line in (root / 'events').read_text().splitlines()]
            requests = [event[event.index('--request-id') + 1] for event in events if '--request-id' in event]
            self.assertEqual(len(requests), 3)
            self.assertEqual(requests[0], requests[1])
            self.assertNotEqual(requests[1], requests[2])
            self.assertTrue(all('--wait' in event for event in events if '--request-id' in event))
            config.chmod(0o644)
            before = (root / 'events').read_bytes()
            result = subprocess.run(['bash', str(ROOT / 'example/ci/deploy.sh')], env=env, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual((root / 'events').read_bytes(), before)

    def test_templates_are_inert_and_share_the_driver(self):
        for filename in ['github-actions.yaml', 'gitlab-ci.yaml']:
            value = yaml.safe_load((ROOT / 'example/ci' / filename).read_text())
            self.assertIsInstance(value, dict)
        for filename in ['github-actions.yaml', 'gitlab-ci.yaml', 'Jenkinsfile']:
            self.assertIn('bash example/ci/deploy.sh', (ROOT / 'example/ci' / filename).read_text())
        self.assertFalse((ROOT / '.github/workflows/deploy.yml').exists())


if __name__ == '__main__':
    unittest.main()
