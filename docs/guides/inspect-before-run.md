# Inspect a project before running it

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow to review the declared manifest, effective merge, and value provenance without booting a VM. The [command surface](../reference/spec/01-command-surface.md) distinguishes these views.

## Inspect the library and binding

```console
$ viv manifest list --json
$ viv manifest show inspect-dev --json
$ viv config --json
```

If the project is not bound yet, preview or write the binding before continuing.

## Evaluate and trace the configuration

```console
$ viv config eval --json
$ viv config sources --json
```

Use [composition and determinism](../reference/spec/04-composition-and-determinism.md) to interpret priority and list merging. For automation, handle only the command-specific outcomes in the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_04_inspect_before_run_usage` checks the declared view and the invocation surface, and `workflow_04_inspect_before_run` checks the effective and provenance views without creating runtime or volume state.
