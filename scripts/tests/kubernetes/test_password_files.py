#!/usr/bin/env python3
"""Reject malformed addon passwords before namespace, Secret or database changes."""
from _paths import EXAMPLE, TOOLS
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import unittest

ROOT = EXAMPLE
SHELL = r'''
set -euo pipefail
# Source the real common functions without preparing host Helm directories.
mkdir() { :; }
source COMMON
unset -f mkdir
# These functions replace every Kubernetes/Helm side effect with a local marker.
k() { printf 'k\n' >> "$CALLS"; }
helm_install() { printf 'helm\n' >> "$CALLS"; }
source INSTALLER
'''


@unittest.skipUnless(os.geteuid() == 0, 'addon password files must be owned by root')
class PasswordFiles(unittest.TestCase):
    def invoke(self, installer, value, mode=0o600):
        with tempfile.TemporaryDirectory(prefix='converyn-password-check-') as directory:
            root = Path(directory)
            password = root / 'synthetic-password'
            password.write_bytes(value)
            password.chmod(mode)
            calls = root / 'calls'
            env = dict(os.environ, HARBOR_PASSWORD_FILE=str(password), GRAFANA_PASSWORD_FILE=str(password), CALLS=str(calls))
            script = SHELL.replace('COMMON', shlex.quote(str(ROOT / 'scripts/common.sh'))).replace('INSTALLER', shlex.quote(str(ROOT / 'scripts' / (installer + '.sh'))))
            result = subprocess.run(['bash', '-c', script], env=env, capture_output=True)
            return result, calls.read_text() if calls.exists() else ''

    def test_valid_private_file_reaches_kubernetes_and_helm_without_printing_password(self):
        for installer in ['harbor', 'monitoring']:
            for value in [b'synthetic-test-value-42', b'synthetic test passphrase 42']:
                with self.subTest(installer=installer, interior_space=b' ' in value):
                    result, calls = self.invoke(installer, value)
                    self.assertEqual(result.returncode, 0)
                    self.assertIn('helm\n', calls)
                    self.assertEqual(result.stdout, b'')
                    self.assertEqual(result.stderr, b'')

    def test_whitespace_empty_nul_and_invalid_utf8_fail_before_any_cluster_change(self):
        values = [b'', b'synthetic-test-value-42\n', b' leading', b'trailing ',
                  b'line\nbreak', b'carriage\rreturn', b'has\0nul', b'\xffinvalid', '\u00a0leading'.encode(), 'trailing\u00a0'.encode()]
        for installer in ['harbor', 'monitoring']:
            for index, value in enumerate(values):
                with self.subTest(installer=installer, case=index):
                    result, calls = self.invoke(installer, value)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(calls, '')
                    self.assertEqual(result.stdout, b'')
                    self.assertEqual(result.stderr, b'password file must contain nonempty UTF-8 text without NUL, CR/LF or leading/trailing whitespace\n')

    def test_publicly_readable_file_fails_before_any_cluster_change(self):
        for installer in ['harbor', 'monitoring']:
            result, calls = self.invoke(installer, b'synthetic-test-value-42', mode=0o644)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(calls, '')
            self.assertEqual(result.stdout, b'')
            self.assertEqual(result.stderr, b'')


if __name__ == '__main__':
    unittest.main(verbosity=2)
