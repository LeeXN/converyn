# Node groups and workflow scope

[English](node-groups.md) | [简体中文](../../zh-CN/how-to/node-groups.md)

Platform node groups bound allowed Agents. Workflow inventory owns deployment topology, business groups and order. Node UIDs connect enrolled identities to both. Nodes may join several platform groups; membership does not guarantee physical isolation between environments.

After installation, personal context import and node-a enrollment in the [quickstart](../tutorials/quickstart.md), a platform operator creates the group:

```bash
converyn --context qa-op update node-groups --file example/node-group.yaml
converyn --context qa-op get node-groups example-hosts
```

Creation uses `create_node_group`, `expected_revision: 0` and revision 1. Updates use `update_node_group`, the current expected revision and the next revision. UID is immutable, name is for display, and node_uids is an unordered set. Empty groups are allowed; execution selections must be nonempty. Members must be enrolled, non-retired Agents.

The example workflow declares both `node_group_uid: example-hosts` and `hosts: all`. `qa` is a user-defined preset name. Selectors resolve only against that workflow's inventory; platform names do not become workflow groups. Adding platform members does not expand deployment nodes.

Initial host registration, adding a host target, changing a binding or renaming a target requires both workflow maintainer and platform operator permissions on the same caller. Super administrators have both. Platform permission alone grants no workflow execution rights. With approved target-name/group-UID bindings unchanged, maintainers may edit in-scope nodes, addresses, hostnames, groups, order and steps. Re-adding a deleted target requires approval again. Removing the last target leaves an unbound workflow that maintainers can inspect; it cannot execute until a new target is approved.

```bash
converyn --context qa-op register workflows --file example/workflow
converyn --context qa-maintainer register workflows --file example/workflow
converyn --context qa-alice run workflows demo --target qa --dry-run
converyn --context qa-alice run workflows demo --target qa --request-id demo-scope-001 --wait
converyn --context qa-alice get runs run-demo-scope-001
converyn --context qa-alice logs runs run-demo-scope-001
```

An access administrator must grant qa-maintainer maintenance rights to demo first. Execution remains authorized by workflow and preset. Overrides and parameter bindings cannot change group bindings. Local validation and offline compilation check structure and selectors only; platform membership remains unverified. The server checks every target at registration, then checks the selected target at preview, submission and post-preparation admission. Out-of-scope errors name the group, target and nodes; selection is never silently filtered.

A changed group revision invalidates a preview before submission. Runs freeze workflow versions, nodes and order. Unrelated membership changes after submission are harmless. Removing a required member during preparation fails admission with scope_invalid and requires a fresh preview/submission. Admitted Runs protect required members while they retain occupancy, including paused, cancellation-pending and needs_attention Runs. Locks use separate Agent and external-target namespaces, never group locks. Disjoint nodes in one group can run concurrently; overlapping groups cannot bypass node locks or resource ownership.

Automatic reconciliation stops retrying on scope_invalid. Repair the scope, then preview/submit or restore the reconciliation configuration as a maintainer. Maintenance, windows, ownership and identity recovery depend on enrollment and execution occupancy. Removing membership preserves identity, maintenance, ownership and history. Node retirement checks every group and active execution. Disaster recovery quarantines every non-retired enrolled Agent, including ungrouped nodes.

```bash
converyn --context qa-op delete node-groups obsolete-hosts --expected-revision 3 --reason 'environment retired'
```

Groups referenced by current workflows, including disabled workflows, cannot be deleted. Deletion retains an audit and retired UID that cannot be reused. Ordinary workflow users see only authorized target inventory, never full platform membership. Pagination contexts bind permissions and scope changes. HTTP uses `GET /api/v1/node-groups` and `POST /api/v1/node-groups/commands`.

The current storage schema is 23, protocol is 27 and Plan format is 4. Old stores, components, deployment fields and clusters commands/APIs are rejected without migration. Control-plane identity, Raft membership and Kubernetes business cluster identities retain their distinct meanings.
