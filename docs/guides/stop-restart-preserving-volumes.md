# Stop and restart while preserving volumes

Use this flow when ending a VM session without discarding the project’s persistent working state. The [workspace and volume model](../reference/spec/06-workspace-and-project-environment.md) owns persistence, and [VM lifecycle](../reference/spec/10-vm-lifecycle.md) owns stop and restart behavior. Stopping removes nothing by design — [the decision separating `stop` from `destroy`](../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) is what makes a warm restart the default outcome rather than a lucky one.

## Inspect and stop

```console
$ viv volume list --json
$ viv stop
$ viv status
```

Run `viv stop` again when scripting if an idempotent cleanup step is useful. Consult the [exit-code matrix](../reference/spec/14-exit-codes.md) for the narrow stop and volume-list failure surfaces.

## Restart and check your data

```console
$ viv start
$ viv exec -- sh -lc 'test -f "$HOME/your-file"'
```

Use a path under a persistent volume for data that must survive the stop/restart cycle. Which volumes exist and who may declare them comes from [the volume model](../decisions/ADR-0019-volume-model.md); the two sizes `viv volume list` reports differ because [volumes are sparse images with a declared ceiling](../decisions/ADR-0037-volume-disk-format-and-reclamation.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_07_volume_list_requires_binding` checks the pre-binding and conflicting-flag surface, and `workflow_07_stop_restart_preserving_volumes` checks the default and named images, stop idempotence, the stopped state, and a warm restart.
