# Destroy a sandbox and choose a cold or warm rebuild

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when teardown should remove runtime history and you need to choose whether volumes survive. Read [VM lifecycle](../reference/spec/10-vm-lifecycle.md) for teardown and [workspace and volumes](../reference/spec/06-workspace-and-project-environment.md) for persistent data. Destroy is deliberately the only destructive verb, and [the decision that drew the teardown boundary](../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) explains what it may and may not reach.

## Choose cold teardown

```console
$ viv destroy --yes
$ viv start
```

After teardown, confirm the binding with `viv config --json`. Destroy removes the vivarium-owned marker while leaving the manifest binding untouched, so the next start is a clean first run — see the [project identity specification](../reference/spec/15-project-identity.md) for what the marker is and which verbs write and remove it, and [why the marker's lifecycle belongs to the lifecycle verbs rather than to `init`](../decisions/ADR-0043-identity-marker-lifecycle.md) for the reasoning behind that split.

## Choose warm teardown

```console
$ viv destroy --keep-volumes --yes
$ viv start
```

Use the keep-volumes form when the next VM should reattach persistent data. A later `viv gc` handles store-wide reclamation; automate both commands according to the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_08_destroy_usage_surface` checks the confirmation requirement and that garbage collection needs no project binding, and `workflow_08_destroy_cold_rebuild` checks the cold and warm variants, marker removal, and the surviving binding.
