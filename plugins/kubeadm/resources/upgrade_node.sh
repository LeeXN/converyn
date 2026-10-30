transaction_preflight
verify_node
require_drained
test "$(api get --raw=/version | python3 -c 'import json,sys;print(json.load(sys.stdin)["gitVersion"])')" = "$VERSION"
"$KUBEADM" upgrade node --config "$CONFIG_PATH" >/dev/null 2>&1
