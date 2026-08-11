# Inspect a project before running it

> Every command in this guide runs today. It is the first guide that does; the rest still describe the target experience. [`../reference/implementation-status.md`](../reference/implementation-status.md) is the source of truth for status, and it is the page to check before relying on anything here.

Use this flow to review the declared manifest, effective merge, and value provenance without booting a VM. The [command surface](../reference/spec/01-command-surface.md) distinguishes these views; [the decision that gathered them into one inspection namespace](../decisions/ADR-0022-config-inspection-namespace.md) explains why they are three commands rather than flags on one.

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

Use [composition and determinism](../reference/spec/04-composition-and-determinism.md) to interpret priority and list merging. The two views also diverge when the merge carries a defect — one refuses, the other still reports — which [the decision treating evaluation-time defects as hard errors](../decisions/ADR-0042-evaluation-time-content-defects.md) explains. For automation, handle only the command-specific outcomes in the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_04_inspect_before_run_usage` checks the declared view and the invocation surface, and `workflow_04_inspect_before_run` checks the effective and provenance views without creating runtime or volume state.
