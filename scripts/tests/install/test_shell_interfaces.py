"""Public Shell contracts: argument validation, cancellation, delivery and output streams."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_transport import terminal_run

ROOT = Path(__file__).resolve().parents[3]
SCRIPTS = ROOT / 'scripts' / 'ops'


class ShellInterfaces(unittest.TestCase):
    def run_script(self, name, *args, env=None):
        return subprocess.run(['bash', str(SCRIPTS / name), *map(str, args)],
                              input='', capture_output=True, text=True, env=env, timeout=30)

    def fixture(self, root):
        binary = root / 'converyn'
        binary.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
args=sys.argv[1:]
if args==['--help']:
 print('fixture help');sys.exit(0)
if 'can-i' in args:sys.exit(0)
if 'get' in args and 'nodes' in args:
 import time
 print(' {\\n      "observed_ms": '+str(int(time.time()*1000))+'\\n }');sys.exit(0)
assert 'bootstrap' in args,args
if os.environ.get('FAIL_BOOTSTRAP'):
 print('fixture bootstrap rejected',file=sys.stderr);sys.exit(2)
output=pathlib.Path(args[args.index('--output')+1])
output.write_text(json.dumps({'token':'private-bootstrap-marker'}))
print('private-bootstrap-marker')
with pathlib.Path(os.environ['BOOTSTRAP_EVENTS']).open('a') as f:f.write('issued\\n')
''')
        binary.chmod(0o700)
        config = root / 'admin.json'
        config.write_text('{}')
        config.chmod(0o600)
        return binary, config, {**os.environ, 'BOOTSTRAP_EVENTS': str(root / 'bootstrap-events')}

    def test_retired_and_irrelevant_options_fail_before_side_effects(self):
        cases = [
            ('install.sh', ['--directory', '/unused']),
            ('install.sh', ['--known-hosts', '/unused']),
            ('install.sh', ['client', '--member', '1=127.0.0.1:8443']),
            ('install.sh', ['deploy', '--bundle', '/unused']),
            ('init.sh', ['import', '--node', 'unused']),
            ('init.sh', ['browser', '--role', 'viewer']),
        ]
        for name, args in cases:
            with self.subTest(name=name, args=args):
                result = self.run_script(name, *args, '--yes', '--non-interactive')
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(result.stdout, '')

    def test_missing_values_and_noninteractive_confirmation_are_input_errors(self):
        for name in ['install.sh', 'init.sh']:
            with self.subTest(name=name):
                for args in [[], ['--binary'], ['--binary', '--yes'], ['--non-interactive']]:
                    result = self.run_script(name, *args)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertEqual(result.stdout, '')
                help_result = self.run_script(name, '-h')
                self.assertEqual(help_result.returncode, 0)
                self.assertEqual(help_result.stderr, '')

    def test_invalid_explicit_binary_does_not_fall_back_to_repository_build(self):
        for name, action in [('install.sh', 'client'), ('init.sh', 'node')]:
            args = [action] if action else []
            result = self.run_script(name, *args, '--binary', '/nonexistent/converyn', '--yes', '--non-interactive')
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(result.stdout, '')

    def test_binary_discovery_prefers_path_then_adjacent_release(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root / 'tools'
            tools.mkdir()
            (tools / 'converyn').symlink_to('/bin/true')
            architecture = 'amd64' if os.uname().machine == 'x86_64' else 'arm64'
            (root / ('converyn-linux-' + architecture)).symlink_to('/bin/echo')
            env = {**os.environ, 'PATH': str(tools) + ':' + os.environ['PATH']}
            for expected in ['/bin/true', '/bin/echo']:
                for name, code in [
                    ('install.sh', 'INSTALL_SCRIPT=$2/install.sh; select_binary; printf "%s" "$BINARY"'),
                    ('init.sh', 'INIT_SCRIPT=$2/init.sh; select_inputs; printf "%s" "$BINARY"'),
                ]:
                    result = subprocess.run(['bash', '-c', 'source "$1"; ' + code,
                                             '_', str(SCRIPTS / name), str(root)],
                                            capture_output=True, text=True, env=env, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(Path(result.stdout.strip()), Path(expected).resolve())
                (tools / 'converyn').unlink(missing_ok=True)

    def test_init_and_install_cancel_before_persistent_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, config, env = self.fixture(root)
            for name, args in [
                ('init.sh', ['node', '--binary', binary, '--admin-config', config,
                             '--node', 'new-node', '--output', root / 'delivery']),
                ('install.sh', ['deploy', '--binary', ROOT / 'target/debug/converyn',
                                '--cluster', 'cancel', '--operator-name', 'OP',
                                '--installation', root / 'installation', '--member', '1=127.0.0.1:8443',
                                '--prepare-only', '--no-input-cache']),
            ]:
                status, text, count = terminal_run(SCRIPTS / name, args, [(b'Proceed', 'no', False)])
                self.assertEqual(status, 1, text)
                self.assertEqual(count, 1, text)
            self.assertFalse((root / 'installation').exists())
            self.assertFalse((root / 'delivery').exists())
            self.assertFalse((root / 'delivery.init-plan.tsv').exists())

    def test_node_enrollment_forwards_ssh_selectors_and_resumes_original_delivery(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, config, env = self.fixture(root)
            tools = root / 'tools'
            tools.mkdir()
            for name in ['ssh', 'scp']:
                tool = tools / name
                tool.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
with pathlib.Path(os.environ['SSH_EVENTS']).open('a') as f:
 f.write(json.dumps([pathlib.Path(sys.argv[0]).name,*sys.argv[1:]])+'\\n')
command=sys.argv[-1]
if 'uname -m' in command:print(os.uname().machine)
elif 'mktemp -d' in command:print('/tmp/converyn-install.FIXTURE123')
else:print('underlying transport output')
''')
                tool.chmod(0o700)
            env.update(PATH=str(tools)+':'+env['PATH'], SSH_EVENTS=str(root/'ssh-events'))
            known = root / 'known_hosts'
            known.write_text('verified fixture host')
            key = root / 'key'
            key.write_text('fixture key selector')
            policy = root / 'policy.json'
            policy.write_text('{"file_paths":["/opt/fixture"]}')
            output = root / 'delivery'
            args = ['node', '--binary', binary, '--admin-config', config, '--node', 'new-node', '--output', output,
                    '--ssh-host', '127.0.0.1', '--ssh-user', 'root', '--ssh-key', key,
                    '--ssh-port', '2222', '--ssh-known-hosts', known,
                    '--hardware-id-file', '/etc/fixture-id', '--yes', '--non-interactive']
            result = self.run_script('init.sh', *args, env=env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.count('Action:'), 1)
            self.assertIn('Heartbeat: verified', result.stdout)
            self.assertNotIn('underlying transport output', result.stdout)
            self.assertNotIn('private-bootstrap-marker', result.stdout + result.stderr)
            self.assertFalse((output/'policy.json').exists())
            original = hashlib.sha256((output/'bootstrap.json').read_bytes()).hexdigest()
            resumed = self.run_script('init.sh', *args, '--resume', env=env)
            self.assertEqual(resumed.returncode, 0, resumed.stderr)
            self.assertEqual(hashlib.sha256((output/'bootstrap.json').read_bytes()).hexdigest(), original)
            self.assertEqual((root/'bootstrap-events').read_text(), 'issued\n')
            events = [json.loads(line) for line in (root/'ssh-events').read_text().splitlines()]
            installs = [event[-1] for event in events if 'bash ' in event[-1] and "'agent'" in event[-1]]
            self.assertEqual(len(installs), 2)
            self.assertNotIn('--policy', installs[0])
            self.assertIn("--hardware-id-file '/etc/fixture-id'", installs[0])
            self.assertIn('--resume', installs[1])
            self.assertTrue(any('2222' in event and 'StrictHostKeyChecking=yes' in event for event in events))

    def test_init_success_summary_and_execution_error_have_distinct_streams_and_codes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, config, env = self.fixture(root)
            for fail in [False, True]:
                output = root / ('failure' if fail else 'success')
                selected_env = {**env, **({'FAIL_BOOTSTRAP': '1'} if fail else {})}
                result = self.run_script('init.sh', 'node', '--binary', binary, '--admin-config', config,
                                         '--node', 'new-node', '--output', output, '--yes', '--non-interactive', env=selected_env)
                if fail:
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertEqual(result.stdout, '')
                    self.assertTrue(output.exists())
                    self.assertTrue(Path(str(output)+'.init-plan.tsv').exists())
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue(result.stdout.startswith('Action: init node\nResult: succeeded\n'))
                    self.assertIn('Node delivery:', result.stderr)
                self.assertNotIn('private-bootstrap-marker', result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
