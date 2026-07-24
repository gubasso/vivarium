# 01 — Command surface

The command-line verbs vivarium exposes. This page is the lookup table for the surface; the intended
end-to-end flow is walked in [`../../guides/getting-started.md`](../../guides/getting-started.md), and
current implementation state is tracked in [`../implementation-status.md`](../implementation-status.md).

All commands operate on the manifest bound to the current project, resolved by the precedence in
[`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md).

## Verbs

| Command | Purpose |
| ------- | ------- |
| `viv init [--manifest <name>] [--write] [--yes] [--json] [--no-input]` | Binding assistant for the current project. Default is read-only: it resolves the effective manifest, lists candidates, and prints the exact registry snippet it *would* record. It persists the project→manifest binding to the **state** registry only with `--write` (confirmed, or `--yes` to skip the prompt). It never writes config and never touches the project's own tree. |
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
  highest-precedence source; `VIVARIUM_MANIFEST` is the next, a runtime override. Neither is
  persisted. The only persisted binding is the state registry (see
  [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)).
- Where no manifest can be resolved, commands that require one **fail closed** — never prompting —
  and print a copy-pasteable snippet plus `viv init` guidance, per
  [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md).
  Interactive prompting happens only in `viv init`, and only when stdin/stdout is a TTY and
  `--no-input` is absent.

## Exit behavior

- Commands return zero on success and non-zero on failure.
- `viv exec` propagates the exit status of the command it ran inside the VM.
- Diagnostics (`doctor`, `config`, `show --resolved`) are read-only and never modify project or VM
  state.
