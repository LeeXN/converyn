set -euo pipefail
# 只允许插件配置档提供的确定参数；所有身份/配置路径可由 Node 本地 grant 再固定。
: "${VERSION:?}" "${CA_SHA256:?}" "${NODE_NAME:?}" "${CA_FILE:?}" "${KUBECONFIG:?}"
: "${CONVERYN_OWNER_UID:?}"
case "$VERSION" in v1.36.4|v1.37.0) ;; *) echo 'unsupported kubeadm profile' >&2; exit 64;; esac
command -v python3 >/dev/null
KUBEADM=/usr/local/bin/kubeadm
KUBECTL=/usr/local/bin/kubectl
export KUBECONFIG
actual_ca=$(sha256sum -- "$CA_FILE"); test "${actual_ca%% *}" = "$CA_SHA256"
api() { "$KUBECTL" --request-timeout=15s "$@"; }
node_json() { api get node "$NODE_NAME" -o json; }
verify_node() {
  # kubeadm can return before kubelet publishes the Node. Wait for registration,
  # while keeping identity/version failures fatal once a Node is observable.
  local node deadline=$((SECONDS + 60))
  until node=$(node_json 2>/dev/null); do
    if test "$SECONDS" -ge "$deadline"; then
      echo "Node registration did not become observable: $NODE_NAME" >&2
      return 1
    fi
    sleep 2
  done
  printf '%s' "$node" | python3 -c '
import json,sys,os
n=json.load(sys.stdin);assert n["metadata"]["name"]==os.environ["NODE_NAME"]
u=open("/sys/class/dmi/id/product_uuid").read().strip().lower()
assert u and n["status"]["nodeInfo"]["systemUUID"].lower()==u,"Node hardware identity mismatch"
assert n["metadata"]["uid"],"Node UID missing"
print(json.dumps({"node_uid":n["metadata"]["uid"],"version":n["status"]["nodeInfo"]["kubeletVersion"]}))'
}
verify_ready() {
 node_json | python3 -c '
import json,sys,os
n=json.load(sys.stdin);assert n["status"]["nodeInfo"]["kubeletVersion"]==os.environ["VERSION"]
assert any(c["type"]=="Ready" and c["status"]=="True" for c in n["status"]["conditions"]),"Node not Ready"'
}
transaction_preflight() {
 : "${BINARY_SHA256:?}" "${CONFIG_PATH:?}" "${CONFIG_SHA256:?}"
 local hash; hash=$(sha256sum -- "$KUBEADM"); test "${hash%% *}" = "$BINARY_SHA256"
 hash=$(sha256sum -- "$CONFIG_PATH"); test "${hash%% *}" = "$CONFIG_SHA256"
 test "$("$KUBEADM" version -o short)" = "$VERSION"
 "$KUBEADM" config validate --config "$CONFIG_PATH" >/dev/null 2>&1
 test -f /sys/fs/cgroup/cgroup.controllers
 if test -n "${IMAGE_ARCHIVE:-}"; then
   hash=$(sha256sum -- "$IMAGE_ARCHIVE"); test "${hash%% *}" = "${IMAGE_ARCHIVE_SHA256:?}"
   /usr/local/bin/ctr --namespace k8s.io images import --digests "$IMAGE_ARCHIVE" >/dev/null 2>&1
 fi
}
require_drained() {
 node_json | python3 -c '
import json,sys,os
n=json.load(sys.stdin);assert n["spec"].get("unschedulable") is True
assert n["metadata"].get("annotations",{}).get("converyn.io/drain-owner")==os.environ["CONVERYN_OWNER_UID"],"drain owner mismatch"
a=tuple(map(int,n["status"]["nodeInfo"]["kubeletVersion"].lstrip("v").split(".")));b=tuple(map(int,os.environ["VERSION"].lstrip("v").split(".")))
assert a<=b and b[1]<=a[1]+1,"downgrade or skipped minor version"'
}
