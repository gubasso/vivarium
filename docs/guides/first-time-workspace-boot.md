# Declare and boot a workspace for the first time

This flow is implemented. See [`../reference/implementation-status.md`](../reference/implementation-status.md) for the status of adjacent commands.

Use this walkthrough when a project directory is not yet declared by a manifest. The command forms are owned by the [command surface](../reference/spec/01-command-surface.md), while selection and boot behavior are owned by [config and XDG layout](../reference/spec/02-config-and-xdg-layout.md) and [VM lifecycle](../reference/spec/10-vm-lifecycle.md).

## Declare the workspace

Add the project tree to the personal manifest that should own its sandbox:

```toml
[[workspaces]]
source = "/home/alice/src/rust-web"
```

From that tree, use `viv config --json` to confirm that the derived resolution source names `rust-web`. A `--manifest rust-web` flag or `VIVARIUM_MANIFEST=rust-web` may select it explicitly, but the declaration is what makes ordinary working-directory resolution durable. vivarium writes nothing into the workspace.

## Start and inspect the sandbox

```console
$ viv start
$ viv status
$ viv shell
```

Starting is detached, idempotent, and non-destructive, so a second `viv start` is safe — see [the decision that fixed those semantics](../decisions/ADR-0013-vm-lifecycle-and-up.md). The manifest name keys the sandbox, and any workspace that manifest declares reaches the same VM. Use [exec and shell](../reference/spec/12-exec-and-shell.md) for the interactive boundary and [exit codes](../reference/spec/14-exit-codes.md) when automating this flow.

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) exercise this sequence: `workflow_01_manifest_workspace_resolution_usage` covers derived resolution and the state boundaries, and `workflow_01_manifest_workspace_resolution_boot` covers the boot itself.
