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
| `viv start [--rebuild \| --no-rebuild] [--generation <n>] [--attach] [--json] [--quiet] [-v\|-vv]` | Resolve the bound manifest, build the VM, and boot it with the working directory mounted — "ensure built and running." Detached by default (boots and returns); idempotent and non-destructive to a running VM. Full behavior in [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv exec [-t|--tty] [-T|--no-tty] [--env KEY[=VAL]]... -- <cmd> [args...]` | Run a command inside the project VM, starting it first if needed; `--` is required and all following args are guest argv. See [`12-exec-and-shell.md`](12-exec-and-shell.md). |
| `viv shell` | Open a login-interactive PTY shell inside the project VM, starting it first if needed. See [`12-exec-and-shell.md`](12-exec-and-shell.md). |
| `viv stop [--force] [-t\|--timeout <secs>] [--json] [--quiet] [-v\|-vv]` | Gracefully stop the project VM (in-guest agent shutdown, falling back to hard poweroff after `--timeout`, default 10 s; `-1` waits indefinitely), preserving persistent volumes and build generations (N18). `--force` powers off immediately (possible data loss) and conflicts with a nonzero `--timeout` (usage error). Idempotent: nothing running → no-op, exit `0`. Full behavior in [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv destroy [-f\|--yes] [--keep-volumes] [--json] [--quiet] [-v\|-vv]` | Tear the project down: graceful stop, unlink **all** build generations, remove **all** persistent volumes and runtime state. Prompts on a TTY; non-interactive runs require `--yes`. `--keep-volumes` preserves volumes. Store paths become reclaimable and are freed only by a later `viv gc`. Never touches the workspace, config, or project binding. Idempotent. See [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv generations <list\|activate\|rollback\|prune> [--json]` | Manage the retained build generations: `list` (number, timestamp, store path), `activate`/`rollback` the current pointer, `prune [--keep <n>] [--older-than <dur>]` under a retention policy, unlinking GC roots. See [`11-generations-and-build-history.md`](11-generations-and-build-history.md). |
| `viv gc` | Run the store garbage collector — a **global, whole-store** sweep that reclaims store paths unreachable from any GC root, vivarium's or not. Project-scoped retention lives in `viv generations prune`. See [`11-generations-and-build-history.md`](11-generations-and-build-history.md). |
| `viv volume <list\|rm> [--json]` | Manage the project's persistent volumes: `list` shows the default and named volumes with mountpoints, declaring layer, and orphans; `rm <name>` / `rm --all` remove volume data. Removal refuses while the VM runs (stop first). Creation is declarative only — volumes are declared in the manifest or pieces and materialized by `start`. See [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md). |
| `viv show --resolved` | Render the fully merged, evaluated configuration for the bound manifest — the "what did my layers produce" view. |
| `viv doctor` | Diagnose the host from the shared probe catalog: virtualization availability, required tooling, and config sanity. `viv start`'s preflight runs the hard subset of this same catalog. |
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
  `manifest show`, `generations list`, `volume list`), a `--json` record in machine mode, and
  **nothing** for side-effect commands whose result is a VM state change (`start`, `stop`,
  `destroy`).
- **stderr carries everything else** — progress, status, prompts, warnings, errors. Progress is shown
  only when stderr is a TTY, so `… --json 2>/dev/null | jq` is always clean.
- `exec`/`shell` pass guest stdio transparently as specified in
  [`12-exec-and-shell.md`](12-exec-and-shell.md); vivarium progress remains on stderr only so guest
  stdout stays pipeable.
- Color is human-only, honoring `NO_COLOR > FORCE_COLOR > isatty`; JSON and non-TTY output are never
  colored.

## Failure and preflight

- Commands return zero on success and a **specific** non-zero code on failure, from the BSD sysexits
  taxonomy (never a generic `1`), per
  [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).
- Commands that need host prerequisites run a **preflight guard** — the hard subset of the shared
  `viv doctor` probe catalog — and refuse **before any side effect**. Each failure reports
  what / where / why / hint plus a stable check id. See
  [`10-vm-lifecycle.md`](10-vm-lifecycle.md) for `viv start`'s preflight.
- `viv exec` and `viv shell` use sysexits for vivarium-origin failures before a guest process starts;
  after the guest command or shell starts, they return its exit status verbatim, with signal deaths
  reported as `128+S`. See [`12-exec-and-shell.md`](12-exec-and-shell.md).
- Diagnostics (`doctor`, `config`, `show --resolved`, `generations list`, `volume list`) are
  read-only and never modify project or VM state.
