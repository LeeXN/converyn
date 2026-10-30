verify_node
verify_ready
patch=$(node_json | python3 -c '
import json,sys,os
n=json.load(sys.stdin);assert n["metadata"].get("annotations",{}).get("converyn.io/drain-owner")==os.environ["CONVERYN_OWNER_UID"],"drain owner mismatch"
print(json.dumps({"metadata":{"resourceVersion":n["metadata"]["resourceVersion"],"annotations":{"converyn.io/drain-owner":None}},"spec":{"unschedulable":False}}))')
api patch node "$NODE_NAME" --type=merge -p "$patch" >/dev/null
