set -euo pipefail
test "$(id -u)" = 0
. /etc/os-release
case "$ID:$VERSION_ID" in
  ubuntu:24.04|rocky:8|rocky:8.*) ;;
  *) echo 'Supported hosts: Ubuntu 24.04 or Rocky Linux 8.' >&2; exit 1 ;;
esac
test "$(uname -m)" = x86_64
kernel_release=$(uname -r)
IFS=. read -r kernel_major kernel_minor _ <<< "$kernel_release"
if ! [[ "$kernel_major" =~ ^[0-9]+$ && "$kernel_minor" =~ ^[0-9]+$ ]] ||
   (( kernel_major < 5 || (kernel_major == 5 && kernel_minor < 10) )); then
  echo "Linux >= 5.10 is required for Calico 3.33; running $kernel_release. Install a reviewed kernel and reboot." >&2
  exit 1
fi
test -f /sys/fs/cgroup/cgroup.controllers || {
  echo 'Unified cgroup v2 is required. Configure host boot settings and reboot; cgroup v1 is not accepted.' >&2; exit 1;
}
if command -v getenforce >/dev/null && test "$(getenforce)" = Enforcing; then
  echo 'SELinux enforcing requires a reviewed container/Calico policy; this example expects an explicitly prepared permissive or disabled lab host.' >&2
  exit 1
fi
test "$(wc -l < /proc/swaps)" = 1
test "$(hostname)" = "$NODE_NAME"
for tool in python3 openssl mount.nfs conntrack iptables socat modprobe; do command -v "$tool" >/dev/null; done
test ! -e /etc/kubernetes/manifests/kube-apiserver.yaml
test ! -e /etc/kubernetes/kubelet.conf
test ! -e /var/lib/etcd/member
test "$(awk '/MemTotal/ { print $2 }' /proc/meminfo)" -ge 3800000
test "$(df --output=avail -k / | tail -1)" -ge 20000000
if systemctl is-active --quiet firewalld || (command -v ufw >/dev/null && ufw status | grep -q '^Status: active'); then
  echo 'Review host firewall rules before deployment; preflight does not disable them.' >&2
  exit 1
fi
printf 'Host preflight passed: %s\n' "$NODE_NAME"
