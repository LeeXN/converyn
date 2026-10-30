#!/usr/bin/env python3
"""Run the example's real server and release scripts in an isolated directory."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request

EXAMPLE = Path(__file__).resolve().parents[3] / 'example/node-demo'


class SiteRelease(unittest.TestCase):
    def test_publish_verify_rollback_and_reject_conflicting_content(self):
        with tempfile.TemporaryDirectory(prefix='converyn-example-site-') as temporary:
            root = Path(temporary)
            root.chmod(0o755)
            data = root / 'data'
            data.mkdir()
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', 0))
                port = listener.getsockname()[1]
            # Change only the deployment location and listener for isolation.
            for name in ('app.py', 'preflight.sh', 'publish.sh', 'verify.sh'):
                text = (EXAMPLE / name).read_text()
                text = text.replace('/opt/converyn-node-demo/data', str(data))
                text = text.replace('18080', str(port))
                (root / name).write_text(text)
            with (root / 'server.log').open('w+') as log:
                server = subprocess.Popen(['python3', str(root / 'app.py')], stdout=log, stderr=log, **({'user': 65534, 'group': 65534} if os.geteuid() == 0 else {}))
                try:
                    url = f'http://127.0.0.1:{port}'
                    deadline = time.monotonic() + 10
                    while True:
                        try:
                            with urllib.request.urlopen(url + '/ready', timeout=1) as response:
                                self.assertTrue(json.load(response)['ready'])
                            break
                        except urllib.error.URLError:
                            if time.monotonic() >= deadline or server.poll() is not None:
                                log.seek(0)
                                self.fail('server did not start: ' + log.read())
                            time.sleep(.05)
                    with self.assertRaises(urllib.error.HTTPError) as error:
                        urllib.request.urlopen(url + '/health', timeout=2)
                    self.assertEqual(error.exception.code, 503)

                    def script(name, version='v1', success=True):
                        result = subprocess.run(['bash', str(root / name)],
                            env={**os.environ, 'VERSION': version}, capture_output=True, text=True, timeout=15, umask=0o077)
                        self.assertEqual(result.returncode == 0, success, result.stderr)
                        return result

                    script('preflight.sh')
                    for version in ('v1', 'v1', 'v2', 'v1'):
                        script('publish.sh', version)
                        result = script('verify.sh', version)
                        self.assertEqual(json.loads(result.stdout)['version'], version)
                    self.assertTrue((data / 'releases/v2/index.html').is_file())
                    self.assertEqual((data / 'releases/v2').stat().st_mode & 0o777, 0o755)
                    self.assertEqual((data / 'releases/v2/index.html').stat().st_mode & 0o777, 0o644)
                    script('verify.sh', 'v2', success=False)
                    script('publish.sh', '../escape', success=False)
                    self.assertFalse((root / 'escape').exists())
                    (data / 'releases/v1/index.html').write_text('conflicting content')
                    script('verify.sh', success=False)
                    script('publish.sh', success=False)
                finally:
                    server.terminate()
                    server.wait(timeout=5)


if __name__ == '__main__':
    unittest.main()
