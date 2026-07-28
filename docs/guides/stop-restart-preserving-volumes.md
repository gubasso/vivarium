# Stop and restart while preserving volumes

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when ending a VM session without discarding the project’s persistent working state. The [workspace and volume model](../reference/spec/06-workspace-and-project-environment.md) owns persistence, and [VM lifecycle](../reference/spec/10-vm-lifecycle.md) owns stop and restart behavior.

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

Use a path under a persistent volume for data that must survive the stop/restart cycle.

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_07_volume_list_requires_binding` checks the pre-binding and conflicting-flag surface, and `workflow_07_stop_restart_preserving_volumes` checks the default and named images, stop idempotence, the stopped state, and a warm restart.
