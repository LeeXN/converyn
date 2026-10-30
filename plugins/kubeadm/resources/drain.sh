verify_node
patch=$(node_json | python3 -c '
import json,sys,os
n=json.load(sys.stdin);a=n["metadata"].get("annotations",{});owner=a.get("converyn.io/drain-owner")
assert owner in (None,os.environ["CONVERYN_OWNER_UID"]),"foreign drain owner"
assert not n["spec"].get("unschedulable") or owner==os.environ["CONVERYN_OWNER_UID"],"foreign cordon"
a["converyn.io/drain-owner"]=os.environ["CONVERYN_OWNER_UID"]
print(json.dumps({"metadata":{"resourceVersion":n["metadata"]["resourceVersion"],"annotations":a},"spec":{"unschedulable":True}}))')
api patch node "$NODE_NAME" --type=merge -p "$patch" >/dev/null
# 不使用 disable-eviction/force/delete-emptydir-data，保留 PDB 与数据保护。
api drain "$NODE_NAME" --ignore-daemonsets --timeout="${DRAIN_TIMEOUT_SECONDS:-300}s" >/dev/null
