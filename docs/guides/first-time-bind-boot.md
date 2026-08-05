# Bind and boot a project for the first time

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this walkthrough when a project has no vivarium binding yet. The command forms are owned by the [command surface](../reference/spec/01-command-surface.md), while boot behavior and identity are owned by the [lifecycle](../reference/spec/10-vm-lifecycle.md) and [project identity](../reference/spec/15-project-identity.md) specifications.

## Preview and record the binding

From the project directory, preview the registry change before approving it:

```console
$ viv init --manifest rust-web --no-input
$ viv config
$ viv init --manifest rust-web --write --yes
$ viv config --json
```

The preview lets you check the selection without changing the binding. Use the configuration lookup to confirm the recorded result. Nothing is persisted until `--write`, and nothing lands in the project tree at all — [the decision keeping the config library read-only and the binding in state](../decisions/ADR-0011-config-read-only-binding-in-state.md) explains where the record goes instead.

## Start and inspect the sandbox

```console
$ viv start
$ viv status
$ viv shell
```

Starting is detached, idempotent, and non-destructive, so a second `viv start` is safe — see [the decision that fixed those semantics](../decisions/ADR-0013-vm-lifecycle-and-up.md). The first start is also what mints the project's identity marker, per [the marker-lifecycle decision](../decisions/ADR-0043-identity-marker-lifecycle.md). Use [exec and shell](../reference/spec/12-exec-and-shell.md) for the interactive boundary and [exit codes](../reference/spec/14-exit-codes.md) when automating this flow.

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) exercise this sequence: `workflow_01_first_time_bind_usage` covers binding and the project-state boundaries, and `workflow_01_first_time_bind_boot` covers the boot itself.
