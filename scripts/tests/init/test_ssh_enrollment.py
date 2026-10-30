"""Opt-in real SSH/systemd enrollment in a disposable container and private controller."""
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[3]


@unittest.skipUnless(os.environ.get('CONVERYN_SSH_LIVE') == '1', 'requires an explicitly selected local systemd/SSH container image')
class SshEnrollment(unittest.TestCase):
    def test_enroll_and_resume_preserve_node_identity_and_original_bootstrap(self):
        image = os.environ['CONVERYN_SYSTEMD_IMAGE']
        binary = Path(os.environ['CONVERYN_TEST_BINARY']).resolve()
        name = 'converyn-enrollment-' + uuid.uuid4().hex[:12]
        server = None
        with tempfile.TemporaryDirectory(prefix='converyn-real-ssh-') as temporary:
            root = Path(temporary)
            def run(*args, check=True, timeout=60):
                result = subprocess.run(list(map(str, args)), text=True, capture_output=True, timeout=timeout)
                if check:
                    self.assertEqual(result.returncode, 0, result.stderr[-2000:])
                return result
            try:
                gateway = run('docker', 'network', 'inspect', 'bridge', '--format', '{{(index .IPAM.Config 0).Gateway}}').stdout.strip()
                with socket.socket() as listener:
                    listener.bind((gateway, 0))
                    port = listener.getsockname()[1]
                run('docker', 'run', '-d', '--name', name, '--privileged', '--cgroupns=private', '--tmpfs', '/run', '--tmpfs', '/tmp:rw,exec,nosuid,nodev', '-p', '127.0.0.1::22', image)
                mapping = run('docker', 'port', name, '22/tcp').stdout.strip()
                ssh_port = mapping.rsplit(':', 1)[1]
                deadline = time.monotonic() + 40
                while True:
                    ready = run('docker', 'exec', name, 'systemctl', 'is-active', 'ssh', check=False)
                    if ready.returncode == 0:
                        break
                    self.assertLess(time.monotonic(), deadline, ready.stderr)
                    time.sleep(.2)
                run('docker', 'exec', name, 'sh', '-c', 'umask 077; printf %s "$1" > /etc/converyn-fixture-identity', 'fixture', uuid.uuid4().hex)
                key = root / 'ssh-key'
                run('ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', key)
                run('docker', 'exec', name, 'install', '-d', '-o', 'deploy', '-g', 'deploy', '-m', '0700', '/home/deploy/.ssh')
                result = subprocess.run(['docker', 'exec', '-i', name, 'sh', '-c', 'cat > /home/deploy/.ssh/authorized_keys && chown deploy:deploy /home/deploy/.ssh/authorized_keys && chmod 600 /home/deploy/.ssh/authorized_keys'], input=key.with_suffix('.pub').read_text(), text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                host_key = run('docker', 'exec', name, 'cat', '/etc/ssh/ssh_host_ed25519_key.pub').stdout.strip()
                known = root / 'known_hosts'
                known.write_text('[127.0.0.1]:' + ssh_port + ' ' + host_key + '\n')
                known.chmod(0o600)
                bundle = root / 'bundle'
                run(binary, 'admin', 'init', '--directory', bundle, '--cluster', 'ssh-enrollment', '--member', f'1={gateway}:{port}')
                with (root / 'controller.log').open('w') as log:
                    server = subprocess.Popen([str(binary), 'server', '--config', str(bundle / 'controller-1/server.json')], stdout=log, stderr=log)
                deadline = time.monotonic() + 30
                while True:
                    initialized = run(binary, '--config', bundle / 'admin/config.json', 'admin', 'initialize', '--members', bundle / 'members.json', check=False)
                    if initialized.returncode == 0:
                        break
                    self.assertLess(time.monotonic(), deadline, initialized.stderr)
                    time.sleep(.2)
                operator = root / 'operator'
                run(binary, '--config', bundle / 'admin/config.json', 'create', 'identities', '--authority', bundle / 'user-authority', '--name', 'SSH fixture operator', '--super-admin', '--output', operator)
                output = root / 'delivery'
                args = ['bash', str(ROOT / 'scripts/ops/init.sh'), 'node', '--binary', str(binary), '--admin-config', str(operator / 'config.json'), '--node', 'ssh-fixture', '--output', str(output), '--ssh-host', '127.0.0.1', '--ssh-user', 'deploy', '--ssh-auth', 'key', '--ssh-key', str(key), '--ssh-port', ssh_port, '--ssh-known-hosts', str(known), '--hardware-id-file', '/etc/converyn-fixture-identity', '--yes', '--non-interactive']
                result = run(*args, timeout=150)
                self.assertIn('Heartbeat: verified', result.stdout)
                self.assertEqual(result.stdout.count('Action:'), 1)
                before = hashlib.sha256((output / 'bootstrap.json').read_bytes()).hexdigest()
                identity = json.loads(run('docker', 'exec', name, 'cat', '/etc/converyn/agent.json').stdout)
                credential_paths = [identity['token_file'], *identity['tls'].values()]
                credentials = run('docker', 'exec', name, 'sha256sum', *credential_paths).stdout
                result = run(*args, '--resume', timeout=150)
                self.assertIn('Heartbeat: verified', result.stdout)
                self.assertEqual(before, hashlib.sha256((output / 'bootstrap.json').read_bytes()).hexdigest())
                # Resume may normalize JSON whitespace; identity and credentials must remain identical.
                resumed = json.loads(run('docker', 'exec', name, 'cat', '/etc/converyn/agent.json').stdout)
                self.assertTrue(identity == resumed, 'Resumed Node configuration semantics changed')
                self.assertEqual(credentials, run('docker', 'exec', name, 'sha256sum', *credential_paths).stdout)
                selected = json.loads(run(binary, '--config', operator / 'config.json', '-o', 'json', 'get', 'nodes', 'ssh-fixture').stdout)
                self.assertIn('ssh-fixture', selected['nodes'])
            finally:
                run('docker', 'rm', '-f', name, check=False)
                if server is not None:
                    server.terminate()
                    server.wait(timeout=10)


if __name__ == '__main__':
    unittest.main()
