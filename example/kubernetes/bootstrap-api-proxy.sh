#!/usr/bin/env bash
# Operator bootstrap for a local, certificate-preserving three-backend API route.
set -euo pipefail
preview=false
if test "${1:-}" = --print; then preview=true; shift; fi
test "$#" = 4 || { echo 'Usage: bootstrap-api-proxy.sh [--print] ENDPOINT_DNS CP1_IPV4 CP2_IPV4 CP3_IPV4' >&2; exit 64; }
template=$(dirname -- "$0")/configs/host/haproxy.cfg
config=$(python3 - "$template" "$@" <<'PY'
import ipaddress, re, sys
from pathlib import Path
template, host, *addresses = sys.argv[1:]
assert re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9.-]*', host)
assert len(set(addresses)) == 3
for address in addresses:
    ipaddress.IPv4Address(address)
backends = '\n'.join('  server cp' + str(i) + ' ' + address + ':6443 check inter 1s fall 2 rise 2'
                     for i, address in enumerate(addresses, 1))
print(Path(template).read_text().replace('${BACKENDS}', backends), end='')
PY
)
if "$preview"; then printf '%s\n' "$config"; exit 0; fi
test "$(id -u)" = 0
. /etc/os-release
case "$ID:$VERSION_ID" in
  ubuntu:24.04|rocky:8|rocky:8.*) ;;
  *) echo 'Supported proxy hosts: Ubuntu 24.04 or Rocky Linux 8.' >&2; exit 1 ;;
esac
# Do not take over an unrelated proxy on an existing host.
if test -e /etc/haproxy/haproxy.cfg; then
  grep -qxF '# Managed Converyn Kubernetes API proxy' /etc/haproxy/haproxy.cfg || {
    echo 'Existing HAProxy configuration is not owned by this example; review it first.' >&2; exit 1;
  }
fi
if test "$ID" = ubuntu; then
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y haproxy
else
  rpm -q haproxy >/dev/null 2>&1 || dnf install -y haproxy
fi
printf '%s\n' "$config" > /etc/haproxy/haproxy.cfg.converyn-next
haproxy -c -f /etc/haproxy/haproxy.cfg.converyn-next
if test -e /etc/haproxy/haproxy.cfg && test ! -e /etc/haproxy/haproxy.cfg.before-converyn; then
  cp -p /etc/haproxy/haproxy.cfg /etc/haproxy/haproxy.cfg.before-converyn
fi
mv /etc/haproxy/haproxy.cfg.converyn-next /etc/haproxy/haproxy.cfg
python3 - "$1" <<'PY'
from pathlib import Path
import re, sys
p = Path('/etc/hosts')
b = p.with_name('hosts.before-converyn-api-proxy')
if not b.exists():
    b.write_text(p.read_text())
text = re.sub(r'\n?# BEGIN CONVERYN API PROXY\n.*?# END CONVERYN API PROXY\n?', '\n', p.read_text(), flags=re.S)
p.write_text(text.rstrip() + '\n# BEGIN CONVERYN API PROXY\n127.0.0.1 ' + sys.argv[1] + '\n# END CONVERYN API PROXY\n')
cloud = Path('/etc/cloud/cloud.cfg.d')
if cloud.is_dir():
    (cloud / '99-converyn-api-proxy.cfg').write_text('manage_etc_hosts: false\n')
PY
systemctl enable haproxy
systemctl restart haproxy
printf 'Local API proxy configured. Kubernetes readiness is verified after init/join.\n'
