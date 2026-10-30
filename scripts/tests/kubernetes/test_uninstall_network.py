"""Keep the API available for CNI DEL and reject foreign CNI credentials."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class UninstallNetwork(unittest.TestCase):
    def exercise(self, foreign=False):
        text = (ROOT / 'example/kubernetes/scripts/uninstall.sh').read_text()
        start = text.index("python3 - <<'PY'", text.index('# Calico DEL'))
        self.assertLess(start, text.index('/usr/local/bin/kubeadm reset'))
        program = text[start:].split('\n', 1)[1].split('\nPY\n', 1)[0]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            ca = b'original cluster CA'
            config = root / 'calico-kubeconfig'
            config.write_text(json.dumps({'clusters': [{'name': 'local', 'cluster': {'server': 'https://10.252.0.1:443', 'certificate-authority-data': base64.b64encode(b'foreign CA' if foreign else ca).decode()}}], 'users': [{'name': 'calico', 'user': {'token': 'fixture-private-marker'}}]}))
            config.chmod(0o600)
            original = config.read_bytes()
            network = root / '10-calico.conflist'
            network.write_text(json.dumps({'name': 'k8s-pod-network', 'plugins': [{'type': 'calico', 'kubernetes': {'k8s_api_root': 'https://10.252.0.1:443', 'kubeconfig': str(config)}}]}))
            events = root / 'events'
            kubectl = root / 'kubectl'
            kubectl.write_text('#!/usr/bin/env python3\nimport os\nprint(open(os.environ["CNI_CONFIG"]).read())\n')
            cri = root / 'crictl'
            cri.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
args=sys.argv[1:];action=args[2]
if action=='info':print(json.dumps({'cniconfig':{'Networks':[{'Config':{'Name':'k8s-pod-network','Source':pathlib.Path(os.environ['CNI_NETWORK']).read_text()}}]}}))
elif action=='pods':print(json.dumps({'items':[{'id':'api'},{'id':'application'}]}))
elif action=='inspectp':print(json.dumps({'status':{'metadata':{'name':args[3],'namespace':'demo'},'linux':{'namespaces':{'options':{'network':'NODE' if args[3]=='api' else 'POD'}}}}}))
else:
 assert args[3]=='application','API sandbox must remain alive during network deletion'
 with pathlib.Path(os.environ['EVENTS']).open('a') as out:out.write(action+' '+args[3]+'\\n')
''')
            kubectl.chmod(0o755)
            cri.chmod(0o755)
            program = program.replace('/etc/cni/net.d/10-calico.conflist', str(network)).replace('/etc/cni/net.d/calico-kubeconfig', str(config)).replace('/usr/local/bin/kubectl', str(kubectl)).replace('/usr/local/bin/crictl', str(cri))
            result = subprocess.run([os.sys.executable, '-c', program], env={**os.environ, 'CNI_CONFIG': str(config), 'CNI_NETWORK': str(network), 'EVENTS': str(events), 'CA_SHA256': hashlib.sha256(ca).hexdigest(), 'CLUSTER_ENDPOINT': 'api.example.test:16443'}, text=True, capture_output=True)
            if foreign:
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(events.exists())
                self.assertEqual(config.read_bytes(), original)
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(events.read_text(), 'stopp application\nrmp application\n')
                actual = json.loads(config.read_text())
                self.assertEqual(actual['clusters'][0]['cluster']['server'], 'https://api.example.test:16443')
                self.assertEqual(json.loads(network.read_text())['plugins'][0]['kubernetes']['k8s_api_root'], 'https://api.example.test:16443')
                self.assertEqual(actual['users'][0]['user']['token'], 'fixture-private-marker')
            self.assertNotIn('fixture-private-marker', result.stdout + result.stderr)

    def test_cni_sandboxes_stop_before_api_and_use_original_endpoint(self):
        self.exercise()

    def test_foreign_cni_ca_is_rejected_without_runtime_mutation(self):
        self.exercise(foreign=True)


if __name__ == '__main__':
    unittest.main()
