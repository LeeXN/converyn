#!/usr/bin/env bash
# Lab NFS backend outside Kubernetes: an explicit client allowlist is mandatory.
set -euo pipefail
test "$(id -u)" = 0
test "$#" -ge 2 || { echo 'Usage: bootstrap-nfs.sh /srv/export NODE_IPV4 ...' >&2; exit 64; }
share=$1; shift
python3 - "$share" "$@" <<'PY'
import ipaddress, re, sys
assert re.fullmatch(r'/srv/[a-zA-Z0-9_-]+', sys.argv[1])
for address in sys.argv[2:]:
    ipaddress.IPv4Address(address)
PY
. /etc/os-release
case "$ID:$VERSION_ID" in
  ubuntu:24.04|rocky:8|rocky:8.*) ;;
  *) echo 'Supported NFS hosts: Ubuntu 24.04 or Rocky Linux 8.' >&2; exit 1 ;;
esac
target=/etc/exports.d/converyn-community.exports
test ! -e "$target" || { echo 'Export already exists; review it rather than overwrite.' >&2; exit 1; }
test ! -e "$share" || { echo 'Use a new export directory; existing storage is not modified.' >&2; exit 1; }
if test "$ID" = ubuntu; then
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y nfs-kernel-server
else
  rpm -q nfs-utils >/dev/null 2>&1 || dnf install -y nfs-utils
fi
install -d -m 0777 "$share"
install -d -m 0755 /etc/exports.d
# This permission model is for an isolated lab. Harbor's init containers need
# ownership changes; production NFS must use a separately reviewed identity model.
printf '%s' "$share" > "$target"
for address in "$@"; do printf ' %s(rw,sync,no_subtree_check,no_root_squash)' "$address" >> "$target"; done
printf '\n' >> "$target"
exportfs -ra
systemctl enable --now nfs-server
