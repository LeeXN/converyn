# People, groups and workflow access

[English](access.md) | [简体中文](../../zh-CN/how-to/access.md)

Converyn uses instance capabilities and workflow grants without project spaces. Deploy separate instances for separate environments, or manage multiple environments using workflows and presets in one instance. A context selects a client connection and identity; it is not a server authorization boundary.

## Defaults and roles

New identities have no business access. Bind a person or group to a role and an optional workflow/preset scope. Grants are additive, with no explicit deny rules. Changes affect subsequent reads and submissions; revoking access does not cancel accepted work. Tasks, receipts, logs, downloads and archived records use the original workflow and preset recorded on the execution.

| Role | Capabilities | Scope |
| --- | --- | --- |
| viewer | Read workflows, runs, receipts and logs | One workflow, optional presets, or all workflows |
| executor | Viewer plus preview, execute and control own runs | Same |
| maintainer | Executor plus registration, enable/disable and control all runs in scope | One or all workflows; all presets required |
| platform_operator | Manage nodes, host groups, external targets and control plane; read platform audit | Instance |
| access_admin | Manage ordinary people, groups, credentials and bindings | Instance |
| super_admin | Full administrative access | Instance |

Platform and access administration do not automatically grant business execution. Only super administrators may modify super-administrator bindings, identities, group memberships or credentials. The last enabled super administrator and its last active credential are protected. Roles are fixed in this version; custom roles and delegated administration are not exposed.

Workflow maintenance can change scripts, destinations and fixed parameters, so treat it as privileged access. Human authorization does not replace Agent path, execution account, plugin connection or destination-side restrictions.

## Create and grant

Create personal credentials using the [certificate guide](certificates.md), then obtain the stable identity ID:

```bash
converyn get identities
converyn auth group release-team --name 'Release team' --member USER_ID
converyn auth grant release-test --group release-team --role executor --workflow deploy --preset test
converyn auth grant workflow-owner --person USER_ID --role maintainer --workflow deploy
converyn auth list
```

Replace IDs, workflows and presets with real values. Person/group selectors accept stable UIDs or unique names; bindings store stable UIDs. Ambiguous names require UIDs. `auth group` replaces the complete membership; it does not append members. Omit `--workflow` for all workflows. Omit `--preset` for all presets, including future additions. Preset scopes require exactly one workflow and cannot be used with maintainer or platform roles.

Access management in the GUI supports adding and enabling/disabling people, group editing, grant previews and revocation. A person added through the GUI has no credentials until `create credentials --identity USER_ID` issues them. Private keys and bearer values never enter replicated controller state.

## Inspect and revoke

```bash
converyn auth doctor
converyn auth status
converyn auth can-i execute --workflow deploy --preset test
converyn auth revoke release-test
```

`can-i` exits 0 when allowed and 1 when denied; it performs no business action. `auth status` identifies effective direct and group bindings. The GUI hides unavailable platform pages, and business lists return only authorized records. View-only presets cannot preview or execute.

Changes use `expected_revision`. Convenience commands fetch it automatically; after a conflict, refresh and review before submitting again. For declarative changes use `converyn auth apply -f access-change.yaml`:

```yaml
action: put_binding
expected_revision: 3
binding:
  uid: release-test
  subject:
    kind: group
    uid: release-team
  role: executor
  workflow: deploy
  presets: [test]
```

Pagination cursors bind filters, identity and authorization state. Restart from the first page after access or membership changes. Log following and downloads recheck authorization on subsequent reads.
