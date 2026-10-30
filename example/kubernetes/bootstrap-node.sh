#!/usr/bin/env bash
# Operator bootstrap only: no SSH keys, credentials or application settings are changed.
set -euo pipefail
test "$(id -u)" = 0
. /etc/os-release
case "$ID:$VERSION_ID" in
  ubuntu:24.04|rocky:8|rocky:8.*) ;;
  *) echo 'Supported hosts: Ubuntu 24.04 or Rocky Linux 8.' >&2; exit 1 ;;
esac
test "$(uname -m)" = x86_64
# Calico 3.33 requires 5.10; enabling cgroup v2 on an older EL8 kernel
# does not satisfy the requirements of the complete pinned stack.
kernel_release=$(uname -r)
IFS=. read -r kernel_major kernel_minor _ <<< "$kernel_release"
if ! [[ "$kernel_major" =~ ^[0-9]+$ && "$kernel_minor" =~ ^[0-9]+$ ]] ||
   (( kernel_major < 5 || (kernel_major == 5 && kernel_minor < 10) )); then
  echo "Linux >= 5.10 is required; running $kernel_release. Install a reviewed kernel and reboot before bootstrap." >&2
  exit 1
fi
test -f /sys/fs/cgroup/cgroup.controllers || {
  echo 'Unified cgroup v2 is required. Configure the host boot settings and reboot before bootstrap.' >&2; exit 1;
}
if command -v getenforce >/dev/null && test "$(getenforce)" = Enforcing; then
  echo 'This example requires an explicitly prepared permissive or disabled SELinux lab host; it does not install an enforcing container/Calico policy.' >&2
  exit 1
fi
if test "$ID" = ubuntu; then
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y ca-certificates python3 openssl \
    nfs-common conntrack socat iptables kmod curl iproute2 ethtool
else
  packages=(ca-certificates python3 openssl nfs-utils conntrack-tools socat iptables kmod curl iproute ethtool)
  if ! rpm -q --whatprovides "${packages[@]}" >/dev/null 2>&1; then dnf install -y "${packages[@]}"; fi
fi
# Reload a dedicated drop-in without restarting the management connection.
if systemctl is-active --quiet NetworkManager; then
  nm_config=/etc/NetworkManager/conf.d/converyn-calico.conf
  if test -e "$nm_config" && ! grep -qxF '# Managed Converyn Calico interfaces' "$nm_config"; then
    echo "Existing $nm_config is not owned by this example; review it first." >&2; exit 1
  fi
  install -d -m 0755 /etc/NetworkManager/conf.d
  cat > "$nm_config" <<'NETWORKMANAGER'
# Managed Converyn Calico interfaces
[keyfile]
unmanaged-devices=interface-name:cali*;interface-name:tunl*;interface-name:vxlan.calico
NETWORKMANAGER
  nmcli general reload
fi
# Agent writable roots are directories; do not grant all of /etc just to edit fstab.
swapoff -a
python3 - <<'PY'
from pathlib import Path
p = Path('/etc/fstab')
text = p.read_text() if p.exists() else ''
p.write_text(''.join('# converyn-example ' + line if len(line.split()) >= 3 and line.split()[2] == 'swap' and not line.lstrip().startswith('#') else line for line in text.splitlines(keepends=True)))
PY
for directory in /opt/converyn-example/{charts,values,manifests,helm,evidence} /opt/cni/bin \
  /etc/converyn-example/kubeadm /etc/containerd /etc/cni/net.d /var/lib/cni \
  /etc/kubernetes/pki /var/lib/kubelet /var/lib/etcd /var/lib/containerd /run/containerd; do
  install -d -m 0755 "$directory"
done
chmod 0700 /etc/converyn-example /etc/converyn-example/kubeadm
test -e /etc/fstab || touch /etc/fstab
printf 'Host dependencies prepared. Enroll the Converyn Agent and prepare its approved platform node group.\n'
