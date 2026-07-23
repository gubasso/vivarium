# 01 — Command surface

The command-line verbs vivarium exposes. This page is the lookup table for the surface; the intended
end-to-end flow is walked in [`../../guides/getting-started.md`](../../guides/getting-started.md), and
current implementation state is tracked in [`../implementation-status.md`](../implementation-status.md).

All commands operate on the manifest bound to the current project, resolved by the precedence in
[`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md).

## Verbs

| Command | Purpose |
| ------- | ------- |
| `viv init [--manifest <name>]` | Bind the current project to a manifest. Writes the repository pointer or a user-registry entry; scaffolds a gitignored personal-override file. |
| `viv images list` | List the available images from the config library. |
| `viv manifest list` | List the available manifests. |
| `viv manifest show <name>` | Show a manifest's resolved image, ordered pieces, and policy knobs. |
| `viv up` | Resolve the bound manifest, build the VM, and boot it with the working directory mounted. |
| `viv exec -- <cmd>` | Run a command inside the running VM (starting it first if needed). |
| `viv shell` | Open an interactive shell inside the running VM (starting it first if needed). |
| `viv down` | Stop the current VM, preserving persistent volumes. |
| `viv show --resolved` | Render the fully merged, evaluated configuration for the bound manifest — the "what did my layers produce" view. |
| `viv doctor` | Diagnose the host: virtualization availability, required tooling, and config sanity. |
| `viv config` | Show effective configuration paths and the resolved manifest binding. |

## Selection and overrides

- `--manifest <name>` overrides the bound manifest for a single invocation and is the
  highest-precedence source (see
  [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)).
- Where no manifest can be resolved, commands that require one **fail closed** with a message to run
  `viv init`, per
  [`../../decisions/ADR-0006-manifest-binding-and-precedence.md`](../../decisions/ADR-0006-manifest-binding-and-precedence.md).

## Exit behavior

- Commands return zero on success and non-zero on failure.
- `viv exec` propagates the exit status of the command it ran inside the VM.
- Diagnostics (`doctor`, `config`, `show --resolved`) are read-only and never modify project or VM
  state.
