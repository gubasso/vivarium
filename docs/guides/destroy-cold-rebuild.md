# Destroy a sandbox and choose a cold or warm rebuild

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when teardown should remove runtime history and you need to choose whether volumes survive. Read [VM lifecycle](../reference/spec/10-vm-lifecycle.md) for teardown and [workspace and volumes](../reference/spec/06-workspace-and-project-environment.md) for persistent data.

## Choose cold teardown

```console
$ viv destroy --yes
$ viv start
```

After teardown, confirm the binding with `viv config --json`. The [project identity specification](../reference/spec/15-project-identity.md) is authoritative: destroy removes the vivarium-owned marker while leaving the manifest binding untouched, so the next start is a clean first run.

**Reconciliation note.** Three older sources still describe teardown as never touching the workspace without carving the marker out: the teardown section of [VM lifecycle](../reference/spec/10-vm-lifecycle.md), the destroy row of the [command surface](../reference/spec/01-command-surface.md), and ADR-0018. All three agree the binding survives; only the marker is in conflict, and the project identity specification governs until they are amended.

## Choose warm teardown

```console
$ viv destroy --keep-volumes --yes
$ viv start
```

Use the keep-volumes form when the next VM should reattach persistent data. A later `viv gc` handles store-wide reclamation; automate both commands according to the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_08_destroy_usage_surface` checks the confirmation requirement and that garbage collection needs no project binding, and `workflow_08_destroy_cold_rebuild` checks the cold and warm variants, marker removal, and the surviving binding.
