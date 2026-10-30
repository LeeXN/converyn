set -euo pipefail
umask 077
: "${UNINSTALL_ACTION:?}" "${CA_SHA256:?}" "${BINARY_SHA256:?}" "${CLUSTER_ENDPOINT:?}"
: "${NODE_NAME:?}" "${NODE_ADDRESS:?}" "${NODE_UID:?}" "${FIRST_CONTROL:?}" "${KUBECONFIG:?}"
export KUBECONFIG
case "$UNINSTALL_ACTION" in check|backup|reset|resume) ;; *) exit 64;; esac
base="/opt/converyn-example/evidence/uninstall/$CA_SHA256/$NODE_UID"
backup_record() {
  test -f "$base/backup-ready" && test ! -L "$base/backup-ready"
  local saved
  saved=$(cat "$base/backup-ready")
  case "$saved" in "$base"/backup-*) ;; *) echo 'invalid backup marker' >&2; return 1;; esac
  test -d "$saved" && test ! -L "$saved"
  for file in kubernetes-config.tgz reference-kubeconfig.json node.json ca-sha256; do
    test -f "$saved/$file" && test ! -L "$saved/$file"
    test "$(stat -c '%u:%a' "$saved/$file")" = '0:600'
  done
  test "$(cat "$saved/ca-sha256")" = "$CA_SHA256"
  printf '%s\n' "$saved"
}
finish_only=false
if test "$UNINSTALL_ACTION" = resume && test ! -f "$KUBECONFIG"; then
  backup=$(backup_record)
  export KUBECONFIG="$backup/reference-kubeconfig.json"
  export UNINSTALL_RESUME_NODE="$backup/node.json"
  finish_only=true
fi
# Every wave checks local identity again; removing later control planes must not
# depend on the API staying available after earlier etcd members leave.
python3 - <<'PY'
import base64, hashlib, json, os, socket, subprocess
from pathlib import Path
assert socket.gethostname().split('.')[0] == os.environ['NODE_NAME'], 'host identity mismatch'
addresses = json.loads(subprocess.check_output(['ip', '-j', 'address', 'show']))
assert any(a.get('local') == os.environ['NODE_ADDRESS'] for link in addresses for a in link.get('addr_info', [])), 'host address mismatch'
config = json.loads(subprocess.check_output(['/usr/local/bin/kubectl', 'config', 'view', '--raw', '--minify', '-o', 'json']))
cluster = config['clusters'][0]['cluster']
assert cluster['server'] == 'https://' + os.environ['CLUSTER_ENDPOINT'], 'cluster endpoint mismatch'
ca = base64.b64decode(cluster['certificate-authority-data']) if 'certificate-authority-data' in cluster else Path(cluster['certificate-authority']).read_bytes()
assert hashlib.sha256(ca).hexdigest() == os.environ['CA_SHA256'], 'cluster CA mismatch'
assert hashlib.sha256(Path('/usr/local/bin/kubeadm').read_bytes()).hexdigest() == os.environ['BINARY_SHA256'], 'kubeadm binary mismatch'
if os.environ.get('UNINSTALL_RESUME_NODE'):
    original = json.loads(Path(os.environ['UNINSTALL_RESUME_NODE']).read_text())
    assert original['metadata']['name'] == os.environ['NODE_NAME']
    assert original['status']['nodeInfo']['systemUUID'].lower() == Path('/sys/class/dmi/id/product_uuid').read_text().strip().lower(), 'hardware identity mismatch'
    manifests = Path('/etc/kubernetes/manifests')
    assert not manifests.exists() or not list(manifests.iterdir()), 'missing live kubeconfig but static manifests remain; inspect before resuming'
    ca_file = Path('/etc/kubernetes/pki/ca.crt')
    assert not ca_file.exists() or hashlib.sha256(ca_file.read_bytes()).hexdigest() == os.environ['CA_SHA256'], 'a different cluster CA is present'
    for port in (6443, 2379, 2380):
        with socket.socket() as connection:
            connection.settimeout(1)
            assert connection.connect_ex(('127.0.0.1', port)) != 0, 'a control-plane service remains without live identity; inspect before resuming'
PY
test -x /usr/local/bin/crictl || { echo "Install the pinned CRI tool from this example before removal" >&2; exit 1; }
if test "$UNINSTALL_ACTION" = check; then
  /usr/local/bin/kubectl --request-timeout=20s get node "$NODE_NAME" -o json | python3 -c '
import json, os, pathlib, sys
n=json.load(sys.stdin)
assert n["metadata"]["name"] == os.environ["NODE_NAME"]
assert n["status"]["nodeInfo"]["systemUUID"].lower() == pathlib.Path("/sys/class/dmi/id/product_uuid").read_text().strip().lower(), "hardware identity mismatch"
assert any(a["type"] == "InternalIP" and a["address"] == os.environ["NODE_ADDRESS"] for a in n["status"]["addresses"]), "Node IP mismatch"
print("Verified node identity and original cluster CA")'
  exit 0
fi
if test "$UNINSTALL_ACTION" = backup; then
  test ! -L "$base"
  mkdir -p "$base"
  backup=$(mktemp -d "$base/backup-XXXXXXXX")
  # These archives contain credentials. They stay root-only on the same host;
  # their bytes are never sent to stdout or embedded in a workflow definition.
  tar -C / --one-file-system -czf "$backup/kubernetes-config.tgz" etc/kubernetes etc/cni/net.d
  for path in /var/lib/kubelet/config.yaml /var/lib/kubelet/kubeadm-flags.env; do
    test ! -e "$path" || cp -p "$path" "$backup/"
  done
  /usr/local/bin/kubectl --request-timeout=20s get node "$NODE_NAME" -o json > "$backup/node.json"
  /usr/local/bin/kubectl config view --raw --minify --flatten -o json > "$backup/reference-kubeconfig.json"
  if test "$FIRST_CONTROL" = true; then
    /usr/local/bin/kubectl --request-timeout=20s get nodes,pv -o json > "$backup/nodes-pv.json"
    /usr/local/bin/kubectl --request-timeout=20s get pvc -A -o json > "$backup/pvc.json"
    /usr/local/bin/helm list -A -o json > "$backup/helm.json"
  fi
  printf '%s\n' "$CA_SHA256" > "$backup/ca-sha256"
  printf '%s\n' "$backup" > "$base/backup-ready"
  printf 'Saved private Kubernetes configuration and node evidence: %s\n' "$backup"
  exit 0
fi
backup=$(backup_record)
test -s "$backup/kubernetes-config.tgz"
test "$(cat "$backup/ca-sha256")" = "$CA_SHA256"
tar -tzf "$backup/kubernetes-config.tgz" >/dev/null
# No Helm uninstall or Kubernetes object deletion is issued: storage provisioners
# must never receive a PVC/PV deletion that could remove the NFS backing data.
systemctl disable --now kubelet.service
# Calico DEL needs its API. Keep host-network control-plane sandboxes alive
# until every CNI-backed sandbox has been removed, including on the last member.
# The original endpoint avoids stale kube-proxy service VIPs during teardown.
python3 - <<'PY'
import base64, hashlib, json, os, subprocess, time
from pathlib import Path
def run(args, timeout=90):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            universal_newlines=True, timeout=timeout)
    assert result.returncode == 0, 'CRI cleanup failed: ' + result.stderr[-1000:]
    return result.stdout
path = Path('/etc/cni/net.d/calico-kubeconfig')
if path.exists():
    assert not path.is_symlink(), 'refuse CNI credential alias'
    owner = path.stat()
    config = json.loads(run(['/usr/local/bin/kubectl', '--kubeconfig', str(path),
                             'config', 'view', '--raw', '--flatten', '-o', 'json']))
    assert len(config['clusters']) == 1, 'unexpected Calico cluster configuration'
    cluster = config['clusters'][0]['cluster']
    assert hashlib.sha256(base64.b64decode(cluster['certificate-authority-data'])).hexdigest() == os.environ['CA_SHA256'], 'Calico belongs to a different cluster'
    cluster['server'] = 'https://' + os.environ['CLUSTER_ENDPOINT']
    temporary = path.with_name('calico-kubeconfig.converyn-next')
    assert not temporary.exists(), 'inspect interrupted CNI credential update'
    temporary.write_text(json.dumps(config))
    temporary.chmod(owner.st_mode & 0o777)
    os.chown(str(temporary), owner.st_uid, owner.st_gid)
    os.replace(str(temporary), str(path))
cri = ['/usr/local/bin/crictl', '--runtime-endpoint', 'unix:///run/containerd/containerd.sock']
network_path = Path('/etc/cni/net.d/10-calico.conflist')
if network_path.exists():
    assert path.exists() and not network_path.is_symlink(), 'incomplete or aliased Calico configuration'
    network = json.loads(network_path.read_text())
    assert network['name'] == 'k8s-pod-network', 'unexpected CNI network'
    plugins = [p for p in network['plugins'] if p['type'] == 'calico']
    assert len(plugins) == 1 and plugins[0]['kubernetes']['kubeconfig'] == str(path), 'unexpected Calico credential reference'
    endpoint = 'https://' + os.environ['CLUSTER_ENDPOINT']
    # This explicit override wins over kubeconfig. Wait for containerd's
    # live CNI configuration to reload before asking it to execute DEL.
    plugins[0]['kubernetes']['k8s_api_root'] = endpoint
    owner = network_path.stat()
    temporary = network_path.with_name('10-calico.conflist.converyn-next')
    assert not temporary.exists(), 'inspect interrupted CNI network update'
    temporary.write_text(json.dumps(network))
    temporary.chmod(owner.st_mode & 0o777)
    os.chown(str(temporary), owner.st_uid, owner.st_gid)
    os.replace(str(temporary), str(network_path))
    deadline = time.monotonic() + 20
    while True:
        info = json.loads(run(cri + ['info']))
        loaded = [n['Config'] for n in info.get('cniconfig', {}).get('Networks', [])
                  if n['Config']['Name'] == network['name']]
        if loaded and any(p.get('type') == 'calico' and p.get('kubernetes', {}).get('k8s_api_root') == endpoint
                          for p in json.loads(loaded[0]['Source'])['plugins']):
            break
        assert time.monotonic() < deadline, 'containerd did not reload the reviewed Calico cleanup endpoint'
        time.sleep(.2)
for pod in json.loads(run(cri + ['pods', '-o', 'json'])).get('items', []):
    status = json.loads(run(cri + ['inspectp', pod['id']]))['status']
    network = status.get('linux', {}).get('namespaces', {}).get('options', {}).get('network')
    if network in ('NODE', 2):
        continue
    assert network in ('POD', 0), 'unknown sandbox network mode; inspect before teardown'
    assert status['metadata']['name'] and status['metadata']['namespace'], 'invalid Kubernetes sandbox identity'
    run(cri + ['stopp', pod['id']], timeout=120)
    run(cri + ['rmp', pod['id']], timeout=120)
print('Removed CNI-backed sandboxes while preserving the API for network cleanup')
PY
# The supervised task has a private mount namespace. kubelet volumes belong to
# PID 1's namespace; reset must unmount those real host mounts, not private copies.
if test "$finish_only" = false; then
  /usr/bin/nsenter --target 1 --mount --root --wd -- /usr/local/bin/kubeadm reset --force --cri-socket unix:///run/containerd/containerd.sock 2>&1 | tee "$backup/kubeadm-reset.log"
# kubeadm can return success after cleanup warnings. Membership failures must stop
# the next control-plane wave before it could lose the remaining etcd quorum.
  if grep -Eiq 'failed to remove.*etcd|failed to get.*etcd|failed to remove.*container|failed to clean|failed to unmount' "$backup/kubeadm-reset.log"; then
  echo 'kubeadm reported incomplete cleanup; inspect the preserved reset log' >&2
  exit 1
  fi
fi
python3 - <<'PY'
import json, os, shutil
from pathlib import Path
# Only Calico's known CNI files and this installation's network allocation are
# removed. Other plugins, image caches and host firewall policy remain intact.
root = Path('/etc/cni/net.d')
assert root.resolve() == root, 'refuse CNI directory alias'
for name in ('10-calico.conflist', 'calico-kubeconfig'):
    path = root / name
    if path.is_symlink():
        raise RuntimeError('refuse CNI symlink: ' + str(path))
    if path.exists():
        if name.endswith('.conflist'):
            config = json.loads(path.read_text())
            assert config.get('name') == 'k8s-pod-network', 'unexpected CNI network'
            assert any(p.get('type') == 'calico' for p in config.get('plugins', [])), 'unexpected CNI plugin'
        path.unlink()
path = Path('/var/lib/cni/networks/k8s-pod-network')
if path.exists():
    assert path.resolve() == path and path.is_dir(), 'refuse CNI path alias'
    shutil.rmtree(path)
PY
/usr/bin/nsenter --target 1 --mount --root --wd -- python3 - <<'PY'
import os, socket, subprocess
from pathlib import Path
assert os.readlink('/proc/self/ns/mnt') == os.readlink('/proc/1/ns/mnt'), 'not in host mount namespace'
for name in ['admin.conf', 'super-admin.conf', 'kubelet.conf', 'bootstrap-kubelet.conf']:
    assert not (Path('/etc/kubernetes') / name).exists(), 'kubeconfig remains: ' + name
manifests = Path('/etc/kubernetes/manifests')
assert not manifests.exists() or not list(manifests.iterdir()), 'static Pod manifests remain'
assert not Path('/var/lib/etcd/member').exists(), 'local etcd data remains'
assert not any(line.split()[4].startswith('/var/lib/kubelet/') for line in Path('/proc/1/mountinfo').read_text().splitlines()), 'host kubelet mounts remain'
tasks = subprocess.check_output(['/usr/local/bin/ctr', '--namespace', 'k8s.io', 'tasks', 'list', '--quiet']).strip()
assert not tasks, 'Kubernetes container tasks remain'
for port in [6443, 2379, 2380, 10250]:
    s = socket.socket(); s.settimeout(1)
    try:
        assert s.connect_ex(('127.0.0.1', port)) != 0, 'Kubernetes port remains: ' + str(port)
    finally:
        s.close()
print('Verified host mounts, container tasks, kubeconfigs, manifests, etcd and API ports removed')
PY
if test "${EXPECTED_ETCD_MEMBERS:-0}" -gt 0; then
  export ETCD_REFERENCE_CONFIG="$backup/reference-kubeconfig.json"
  python3 - <<'PY'
import json, os, subprocess, time
expected = int(os.environ['EXPECTED_ETCD_MEMBERS'])
observer = os.environ['ETCD_OBSERVER']
command = ['/usr/local/bin/kubectl', '--kubeconfig', os.environ['ETCD_REFERENCE_CONFIG'], '--request-timeout=15s',
           '-n', 'kube-system', 'exec', 'etcd-' + observer, '--', 'etcdctl',
           '--endpoints=https://127.0.0.1:2379', '--cacert=/etc/kubernetes/pki/etcd/ca.crt',
           '--cert=/etc/kubernetes/pki/etcd/healthcheck-client.crt',
           '--key=/etc/kubernetes/pki/etcd/healthcheck-client.key', 'member', 'list', '--write-out=json']
deadline = time.monotonic() + 90
while time.monotonic() < deadline:
    try:
        result = json.loads(subprocess.check_output(command, stderr=subprocess.DEVNULL, timeout=20))
        members = result['members']
        if len(members) == expected and any(m.get('name') == observer for m in members):
            print('Verified remaining etcd members: ' + str(expected))
            break
    except (subprocess.SubprocessError, ValueError, KeyError):
        pass
    time.sleep(2)
else:
    raise RuntimeError('etcd membership did not shrink; later reset waves must stop')
PY
fi
if systemctl is-active --quiet kubelet.service; then echo 'kubelet is still active' >&2; exit 1; fi
if systemctl is-enabled --quiet kubelet.service; then echo 'kubelet is still enabled' >&2; exit 1; fi
printf 'Kubernetes reset complete; NFS data, containerd/images and Converyn state preserved.\n' | tee "$backup/result.txt"
