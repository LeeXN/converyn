transaction_preflight
verify_node
require_drained
# 参考端点必须指向已经升级的首控制面，不能用仍为旧版本的本地 API 证明前置条件。
test "$("$KUBECTL" --kubeconfig "${REFERENCE_KUBECONFIG:?}" --request-timeout=15s get --raw=/version | python3 -c 'import json,sys;print(json.load(sys.stdin)["gitVersion"])')" = "$VERSION"
"$KUBEADM" upgrade node --config "$CONFIG_PATH" >/dev/null 2>&1
test "$(api get --raw=/version | python3 -c 'import json,sys;print(json.load(sys.stdin)["gitVersion"])')" = "$VERSION"
