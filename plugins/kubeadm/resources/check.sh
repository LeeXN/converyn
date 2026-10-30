# 服务进入 active 不等于 API/Node 已就绪；只重试观察，不重放任何升级事务。
deadline=$((SECONDS + ${CHECK_TIMEOUT_SECONDS:-300}))
until (
 verify_node >/dev/null &&
 verify_ready &&
 test "$(api get --raw=/version | python3 -c 'import json,sys;print(json.load(sys.stdin)["gitVersion"])')" = "$VERSION"
) >/dev/null 2>&1; do
 if test "$SECONDS" -ge "$deadline"; then
  echo 'Kubernetes Node/API readiness check exceeded its bounded deadline' >&2
  exit 1
 fi
 sleep 2
done
verify_node
