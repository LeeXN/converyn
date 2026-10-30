transaction_preflight
verify_node
require_drained
"$KUBEADM" upgrade apply "$VERSION" --yes --config "$CONFIG_PATH" >/dev/null 2>&1
test "$(api get --raw=/version | python3 -c 'import json,sys;print(json.load(sys.stdin)["gitVersion"])')" = "$VERSION"
