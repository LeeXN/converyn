set -euo pipefail
export KUBECONFIG=/etc/kubernetes/admin.conf
export HELM_CACHE_HOME=/opt/converyn-example/helm/cache
export HELM_CONFIG_HOME=/opt/converyn-example/helm/config
export HELM_DATA_HOME=/opt/converyn-example/helm/data
mkdir -p "$HELM_CACHE_HOME" "$HELM_CONFIG_HOME" "$HELM_DATA_HOME"
k() { /usr/local/bin/kubectl --request-timeout=20s "$@"; }
retry() {
  local remaining=$1; shift
  until "$@"; do
    remaining=$((remaining - 1)); test "$remaining" -gt 0 || return 1
    sleep 5
  done
}
helm_install() {
  # A failed/uncertain release is retained for inspection; no automatic uninstall.
  local release=$1 chart=$2 namespace=$3 values=$4
  /usr/local/bin/helm upgrade --install "$release" "$chart" --namespace "$namespace" \
    --create-namespace --values "$values" --wait --wait-for-jobs --timeout 15m --history-max 10
}
require_private_password_file() {
  test -f "$1"
  test "$(stat -c '%u:%a' "$1")" = 0:600
  # kubectl --from-file preserves every byte. Reject padding and line breaks
  # rather than trimming a value a database would store as another password.
  python3 - "$1" <<'PYTHON_PASSWORD'
from pathlib import Path
import sys
try:
    value = Path(sys.argv[1]).read_bytes().decode('utf-8')
    if not value or any(c in value for c in '\0\r\n') or value != value.strip():
        raise ValueError()
except (OSError, UnicodeError, ValueError):
    raise SystemExit('password file must contain nonempty UTF-8 text without NUL, CR/LF or leading/trailing whitespace')
PYTHON_PASSWORD
}
