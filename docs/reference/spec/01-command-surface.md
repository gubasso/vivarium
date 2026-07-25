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
| `viv images list [--json]` | List the images in the config library (`$XDG_CONFIG_HOME/vivarium/images/`). Read-only enumeration; an empty or absent library lists zero rows and exits `0`, never an error. |
| `viv manifest list [--json]` | List the manifests in the config library (`manifests/`). Enumerates all *defined* manifests, not the one bound to the current project — the binding is shown by `viv config`. Empty/absent library lists zero rows and exits `0`. |
| `viv manifest show <name> [--json]` | Show the named manifest's declared image, ordered pieces, and policy knobs, read from the config library. Takes exactly one `<name>`; an unknown name **fails closed** with a non-zero exit. Distinct from `show --resolved`, which evaluates the full module merge. |
| `viv up [--rebuild \| --no-rebuild] [--generation <n>] [--attach] [--json] [--quiet] [-v\|-vv]` | Resolve the bound manifest, build the VM, and boot it with the working directory mounted. Detached by default (boots and returns); idempotent and non-destructive to a running VM. Full behavior in [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv exec -- <cmd>` | Run a command inside the running VM (starting it first if needed). |
| `viv shell` | Open an interactive shell inside the running VM (starting it first if needed). |
| `viv down` | Stop the current VM, preserving persistent volumes. |
| `viv generations [--json]` | List the retained build generations for the project — number, timestamp, store path. See [`11-generations-and-build-history.md`](11-generations-and-build-history.md). |
| `viv gc [--older-than <dur>] [--keep <n>]` | Prune old build generations under a retention policy, unlinking their GC roots. See [`11-generations-and-build-history.md`](11-generations-and-build-history.md). |
| `viv show --resolved` | Render the fully merged, evaluated configuration for the bound manifest — the "what did my layers produce" view. |
| `viv doctor` | Diagnose the host from the shared probe catalog: virtualization availability, required tooling, and config sanity. `viv up`'s preflight runs the hard subset of this same catalog. |
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

## Output streams

The stream and machine-output rules are the same for every command, specified in
[`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md):

- **stdout carries the result only** — a human table/line for data commands (`images list`,
  `manifest show`, `generations`), a `--json` record in machine mode, and **nothing** for
  side-effect commands whose result is a running VM (`up`, `down`).
- **stderr carries everything else** — progress, status, prompts, warnings, errors. Progress is shown
  only when stderr is a TTY, so `… --json 2>/dev/null | jq` is always clean.
- Color is human-only, honoring `NO_COLOR > FORCE_COLOR > isatty`; JSON and non-TTY output are never
  colored.

## Failure and preflight

- Commands return zero on success and a **specific** non-zero code on failure, from the BSD sysexits
  taxonomy (never a generic `1`), per
  [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).
- Commands that need host prerequisites run a **preflight guard** — the hard subset of the shared
  `viv doctor` probe catalog — and refuse **before any side effect**. Each failure reports
  what / where / why / hint plus a stable check id. See
  [`10-vm-lifecycle.md`](10-vm-lifecycle.md) for `viv up`'s preflight.
- `viv exec` propagates the exit status of the command it ran inside the VM.
- Diagnostics (`doctor`, `config`, `show --resolved`, `generations`) are read-only and never modify
  project or VM state.
