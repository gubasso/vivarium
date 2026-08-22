# Destroy a sandbox and choose a cold or warm rebuild

Use this flow when teardown should remove runtime history and you need to choose whether volumes survive. Read [VM lifecycle](../reference/spec/10-vm-lifecycle.md) for teardown and [workspace and volumes](../reference/spec/06-workspace-and-project-environment.md) for persistent data. Destroy is deliberately the only destructive verb, and [the decision that drew the teardown boundary](../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) explains what it may and may not reach.

## Choose cold teardown

```console
$ viv destroy --yes
$ viv start
```

After teardown, confirm workspace resolution with `viv config --json`. Destroy removes vivarium-owned state and data for the selected manifest while leaving the manifest and workspace untouched, so the next start is a clean first run. [Config and XDG layout](../reference/spec/02-config-and-xdg-layout.md) owns the manifest-keyed paths, and [VM lifecycle](../reference/spec/10-vm-lifecycle.md) owns the teardown boundary.

## Choose warm teardown

```console
$ viv destroy --keep-volumes --yes
$ viv start
```

Use the keep-volumes form when the next VM should reattach persistent data. A later `viv gc` handles store-wide reclamation; automate both commands according to the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_08_destroy_usage_surface` checks the confirmation requirement and manifest-keyed scope, and `workflow_08_destroy_cold_rebuild` checks the cold and warm variants, clean workspace, and surviving derived resolution.
