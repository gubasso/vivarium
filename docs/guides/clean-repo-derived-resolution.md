# Keep a workspace clean with derived resolution

This flow is implemented. See [`../reference/implementation-status.md`](../reference/implementation-status.md) for the status of adjacent commands.

Use this flow when the repository should carry no vivarium configuration. The [XDG layout and derived workspace index](../reference/spec/02-config-and-xdg-layout.md) specify where personal manifests and regenerable ownership data belong.

## Declare the workspace outside the repository

```toml
[[workspaces]]
source = "/home/alice/src/clean-repository"
```

Put that block in the personal manifest library, outside the repository, then run `viv config --json` from the declared tree. The command reports `derived` as its source unless a `--manifest` flag or `VIVARIUM_MANIFEST` override is in force. The cache-root workspace index may be deleted at any time; the next resolving command rebuilds it from manifests. There is no project-local file to add or ignore.

## Start the project

```console
$ viv start
$ viv status
```

The selected manifest's name keys the sandbox. Two disjoint workspace trees declared by that manifest therefore reach the same VM, while two manifests claiming the same tree are refused with both names.

## Acceptance coverage

The `workflow_02_derived_workspace_index` trial in [`local_workflows.rs`](../../tests/local_workflows.rs) checks the clean tree, cache rebuild and invalidation, mount exclusion, and two-owner refusal.
