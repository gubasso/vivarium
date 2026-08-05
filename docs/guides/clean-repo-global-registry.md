# Keep a project clean with a global registry binding

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when the repository should carry no vivarium configuration. The [XDG layout and registry](../reference/spec/02-config-and-xdg-layout.md) specify where authored configuration and machine-local bindings belong, and the [command surface](../reference/spec/01-command-surface.md) specifies the binding assistant.

## Bind without adding project configuration

```console
$ viv init --manifest clean-registry --no-input
$ viv init --manifest clean-registry --write --yes
$ viv config --json
```

Review the project tree after each command, then use `viv config --json` to inspect the binding: it is the supported view, and unlike the file on disk it also reflects a `--manifest` or `VIVARIUM_MANIFEST` override in force. The registry's own `[[projects]]` record is a documented shape you may read or hand-edit if you prefer — `viv init` prints exactly that block, and names `--write` beside it if you would rather vivarium wrote it for you. The rest of the state root is private and unspecified; don't build on it. See [the state-root layout](../reference/spec/02-config-and-xdg-layout.md). There is no per-project binding file to add or gitignore: [the decision that moved the binding into the state registry](../decisions/ADR-0011-config-read-only-binding-in-state.md) removed that concept entirely.

## Start the project

```console
$ viv start
$ viv status
```

The [project identity specification](../reference/spec/15-project-identity.md) explains how separate projects with the same directory name remain distinct; [the decision to key identity by name rather than by a path hash](../decisions/ADR-0029-project-identity-and-marker.md) explains why a project survives being moved or renamed.

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_02_clean_repo_global_registry_only` checks the clean-tree and registry flow, and `workflow_02_identity_collision_suffix` checks how a second project with the same name gets its identity.
