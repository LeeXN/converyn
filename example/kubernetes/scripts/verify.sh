# Functional evidence is retained under a unique ID; a timeout is a failed check.
python3 - <<'PY'
import base64
import hashlib
from http.client import HTTPSConnection
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import time
import urllib.parse
import uuid

run = 'check-' + uuid.uuid4().hex[:12]
namespace = 'converyn-verification'
base = ['/usr/local/bin/kubectl', '--request-timeout=20s']
def k(*args, body=None):
    return subprocess.check_output(base + list(args), input=body, timeout=60).decode()
def get(*args):
    return json.loads(k(*args, '-o', 'json'))
def apply(obj):
    k('apply', '-f', '-', body=json.dumps(obj).encode())
def wait(check, seconds=300):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(3)
    raise TimeoutError('functional verification timed out')

nodes = get('get', 'nodes')['items']
assert len(nodes) == int(os.environ['EXPECTED_NODES'])
assert all(any(v['type'] == 'Ready' and v['status'] == 'True' for v in n['status']['conditions'])
           and n['status']['nodeInfo']['kubeletVersion'] == 'v1.36.4' for n in nodes)
workers = [n['metadata']['name'] for n in nodes if 'node-role.kubernetes.io/control-plane' not in n['metadata']['labels']]
controls = [n['metadata']['name'] for n in nodes if 'node-role.kubernetes.io/control-plane' in n['metadata']['labels']]
assert len(controls) == int(os.environ['EXPECTED_CONTROL_PLANES'])
assert len(workers) >= 2, 'cross-node acceptance requires at least two independent workers'
apply({'apiVersion': 'v1', 'kind': 'Namespace', 'metadata': {'name': namespace}})
image = os.environ['VERIFY_IMAGE']
labels = {'converyn.io/verification': run}
def pod(name, node, command, volume=False):
    spec = {'nodeName': node, 'restartPolicy': 'Never', 'terminationGracePeriodSeconds': 1,
            'containers': [{'name': 'check', 'image': image,
            'command': ['sh', '-ec', command]}]}
    if volume:
        spec['containers'][0]['volumeMounts'] = [{'name': 'data', 'mountPath': '/data'}]
        spec['volumes'] = [{'name': 'data', 'persistentVolumeClaim': {'claimName': run}}]
    return {'apiVersion': 'v1', 'kind': 'Pod', 'metadata': {'name': name, 'namespace': namespace, 'labels': labels}, 'spec': spec}
def completed(name):
    p = get('-n', namespace, 'get', 'pod', name)
    if p['status']['phase'] == 'Failed':
        raise RuntimeError('verification pod failed: ' + name)
    return p['status']['phase'] == 'Succeeded'
server = run + '-server'
apply(pod(server, workers[0], 'mkdir -p /www; echo community-network-ok > /www/index.html; httpd -f -p 8080 -h /www'))
wait(lambda: get('-n', namespace, 'get', 'pod', server)['status'].get('podIP'))
server_ip = get('-n', namespace, 'get', 'pod', server)['status']['podIP']
client = run + '-network'
apply(pod(client, workers[-1], 'nslookup kubernetes.default.svc.cluster.local; wget -T 15 -qO- http://' + server_ip + ':8080 | grep community-network-ok'))
wait(lambda: completed(client))
apply({'apiVersion': 'networking.k8s.io/v1', 'kind': 'NetworkPolicy',
       'metadata': {'name': run, 'namespace': namespace}, 'spec': {
           'podSelector': {'matchLabels': {'converyn.io/server': run}}, 'policyTypes': ['Ingress'],
           'ingress': [{'from': [{'podSelector': {'matchLabels': {'converyn.io/allow': run}}}],
                        'ports': [{'port': 8080, 'protocol': 'TCP'}]}]}})
k('-n', namespace, 'label', 'pod', server, 'converyn.io/server=' + run)
time.sleep(5)
denied = run + '-denied'
apply(pod(denied, workers[-1], 'if wget -T 5 -qO- http://' + server_ip + ':8080; then exit 1; fi'))
wait(lambda: completed(denied))
allowed = run + '-allowed'
obj = pod(allowed, workers[-1], 'wget -T 15 -qO- http://' + server_ip + ':8080 | grep community-network-ok')
obj['metadata']['labels'] = dict(labels, **{'converyn.io/allow': run})
apply(obj)
wait(lambda: completed(allowed))
apply({'apiVersion': 'v1', 'kind': 'PersistentVolumeClaim', 'metadata': {'name': run, 'namespace': namespace, 'labels': labels},
       'spec': {'storageClassName': os.environ['STORAGE_CLASS'], 'accessModes': ['ReadWriteMany'],
                'resources': {'requests': {'storage': '1Gi'}}}})
wait(lambda: get('-n', namespace, 'get', 'pvc', run)['status']['phase'] == 'Bound')
pvc = get('-n', namespace, 'get', 'pvc', run)
writer = run + '-writer'
apply(pod(writer, workers[0], 'printf ' + run + ' > /data/proof; sync', volume=True))
wait(lambda: completed(writer))
k('-n', namespace, 'delete', 'pod', writer, '--wait=true', '--timeout=30s')
reader = run + '-reader'
apply(pod(reader, workers[-1], 'test "$(cat /data/proof)" = ' + run, volume=True))
wait(lambda: completed(reader))
pv = get('get', 'pv', pvc['spec']['volumeName'])
assert pv['spec']['csi']['driver'] == 'nfs.csi.k8s.io'

def metrics_ready():
    raw = k('get', '--raw', '/api/v1/namespaces/monitoring/services/http:monitoring-kube-prometheus-prometheus:9090/proxy/api/v1/query?query=up')
    query = json.loads(raw)
    assert query['status'] == 'success'
    up = [v for v in query['data']['result'] if float(v['value'][1]) == 1]
    jobs = sorted({v['metric'].get('job', '') for v in up})
    return jobs if all(any(expected in job for job in jobs) for expected in ['node-exporter', 'kube-state-metrics', 'apiserver']) else None
jobs = wait(metrics_ready)

ca = base64.b64decode(k('-n', 'cert-manager', 'get', 'secret', 'community-root',
                       '-o', 'jsonpath={.data.tls\\.crt}')).decode()
context = ssl.create_default_context(cadata=ca)
host, port = os.environ['HARBOR_HOST'], int(os.environ['HTTPS_NODEPORT'])
password = Path(os.environ['HARBOR_PASSWORD_FILE']).read_text().strip()
basic = 'Basic ' + base64.b64encode(('admin:' + password).encode()).decode()
def http(method, path, data=None, headers=None):
    conn = HTTPSConnection(host, port, context=context, timeout=30)
    conn.sock = context.wrap_socket(socket.create_connection((os.environ['CONTROL_ADDRESS'], port), 30), server_hostname=host)
    conn.request(method, path, body=data, headers=headers or {})
    r = conn.getresponse()
    value = r.read()
    result = r.status, dict(r.getheaders()), value
    conn.close()
    return result
assert http('GET', '/api/v2.0/ping')[0] == 200
assert http('GET', '/v2/')[0] == 401
status, _, _ = http('POST', '/api/v2.0/projects', json.dumps({'project_name': 'community-smoke', 'public': False}),
                   {'Authorization': basic, 'Content-Type': 'application/json'})
assert status in (201, 409), status
scope = urllib.parse.urlencode({'service': 'harbor-registry', 'scope': 'repository:community-smoke/health:pull,push'})
status, _, body = http('GET', '/service/token?' + scope, headers={'Authorization': basic})
assert status == 200
auth = {'Authorization': 'Bearer ' + json.loads(body)['token']}
config = json.dumps({'architecture': 'amd64', 'os': 'linux', 'config': {}, 'rootfs': {'type': 'layers', 'diff_ids': []}}, separators=(',', ':')).encode()
config_sha = 'sha256:' + hashlib.sha256(config).hexdigest()
status, headers, _ = http('POST', '/v2/community-smoke/health/blobs/uploads/', headers=auth)
assert status == 202, status
location = next(v for key, v in headers.items() if key.lower() == 'location')
parsed = urllib.parse.urlsplit(location)
assert not parsed.netloc or (parsed.hostname == host and parsed.port in (None, port))
path = parsed.path + '?' + (parsed.query + '&' if parsed.query else '') + urllib.parse.urlencode({'digest': config_sha})
status, _, _ = http('PUT', path, config, dict(auth, **{'Content-Type': 'application/octet-stream'}))
assert status == 201, status
media = 'application/vnd.oci.image.manifest.v1+json'
manifest = json.dumps({'schemaVersion': 2, 'mediaType': media, 'config': {
    'mediaType': 'application/vnd.oci.image.config.v1+json', 'size': len(config), 'digest': config_sha}, 'layers': []}, separators=(',', ':')).encode()
manifest_sha = 'sha256:' + hashlib.sha256(manifest).hexdigest()
status, _, _ = http('PUT', '/v2/community-smoke/health/manifests/' + run, manifest, dict(auth, **{'Content-Type': media}))
assert status == 201, status
status, _, actual = http('GET', '/v2/community-smoke/health/manifests/' + manifest_sha, headers=dict(auth, Accept=media))
assert status == 200 and hashlib.sha256(actual).hexdigest() == manifest_sha.split(':')[1]
status, _, actual = http('GET', '/v2/community-smoke/health/blobs/' + config_sha, headers=auth)
assert status == 200 and actual == config

evidence = {'passed': True, 'verification_id': run, 'nodes': [n['metadata']['name'] for n in nodes],
    'control_planes': controls, 'workers': workers,
    'network': {'dns': True, 'pod_http': True, 'network_policy_enforced': True, 'server_node': workers[0], 'client_node': workers[-1], 'cross_node': workers[0] != workers[-1]},
    'storage': {'pvc_uid': pvc['metadata']['uid'], 'pv': pv['metadata']['name'], 'driver': 'nfs.csi.k8s.io', 'persisted_after_writer_deleted': True},
    'prometheus_up_jobs': jobs, 'harbor': {'tls_verified': True, 'push_pull_manifest': manifest_sha, 'push_pull_blob': config_sha}}
directory = Path('/opt/converyn-example/evidence')
directory.mkdir(exist_ok=True)
(directory / (run + '.json')).write_text(json.dumps(evidence, indent=2) + '\n')
# Retain the PVC, reader and evidence. Only the temporary HTTP server is removed.
k('-n', namespace, 'delete', 'pod', server, '--wait=true', '--timeout=30s')
print(json.dumps(evidence, indent=2))
PY
