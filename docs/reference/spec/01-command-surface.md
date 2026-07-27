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
| `viv manifest show <name> [--json]` | Show the named manifest's declared image, ordered pieces, and policy knobs, read from the config library. Takes exactly one `<name>`; an unknown name **fails closed** with a non-zero exit. Distinct from `config eval`, which evaluates the full module merge. |
| `viv start [--rebuild \| --no-rebuild] [--generation <n>] [--attach] [--json]` | Resolve the bound manifest, build the VM, and boot it with the working directory mounted — "ensure built and running." Detached by default (boots and returns); idempotent and non-destructive to a running VM. Full behavior in [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv exec [-t|--tty] [-T|--no-tty] [--env KEY[=VAL]]... -- <cmd> [args...]` | Run a command inside the project VM, starting it first if needed; `--` is required and all following args are guest argv. See [`12-exec-and-shell.md`](12-exec-and-shell.md). |
| `viv shell` | Open a login-interactive PTY shell inside the project VM, starting it first if needed. See [`12-exec-and-shell.md`](12-exec-and-shell.md). |
| `viv stop [--force] [-t\|--timeout <secs>] [--json]` | Gracefully stop the project VM (in-guest agent shutdown, falling back to hard poweroff after `--timeout`, default 10 s; `-1` waits indefinitely), preserving persistent volumes and build generations (N18). `--force` powers off immediately (possible data loss) and conflicts with a nonzero `--timeout` (usage error). Idempotent: nothing running → no-op, exit `0`. Full behavior in [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv destroy [-f\|--yes] [--keep-volumes] [--json]` | Tear the project down: graceful stop, unlink **all** build generations, remove **all** persistent volumes and runtime state. Prompts on a TTY; non-interactive runs require `--yes`. `--keep-volumes` preserves volumes. Store paths become reclaimable and are freed only by a later `viv gc`. Never touches the workspace, config, or project binding. Idempotent. See [`10-vm-lifecycle.md`](10-vm-lifecycle.md). |
| `viv generations <list\|activate\|rollback\|prune> [--json]` | Manage the retained build generations: `list` (number, timestamp, store path), `activate`/`rollback` the current pointer, `prune [--keep <n>] [--older-than <dur>]` under a retention policy, unlinking GC roots. See [`11-generations-and-build-history.md`](11-generations-and-build-history.md). |
| `viv gc` | Run the store garbage collector — a **global, whole-store** sweep that reclaims store paths unreachable from any GC root, vivarium's or not. Project-scoped retention lives in `viv generations prune`. See [`11-generations-and-build-history.md`](11-generations-and-build-history.md). |
| `viv volume <list\|rm> [--json]` | Manage the project's persistent volumes: `list` shows the default and named volumes with mountpoints, declaring layer, and orphans; `rm <name>` / `rm --all` remove volume data. Removal refuses while the VM runs (stop first). Creation is declarative only — volumes are declared in the manifest or pieces and materialized by `start`. See [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md). |
| `viv config [--json]` | Inspection namespace for the bound project's configuration (ADR-0022). With no subcommand, shows the binding: the bound manifest and the effective config/state/data/cache paths. Read-only, no VM preflight. |
| `viv config sources [--json]` | Provenance view: the declaring manifest and its ordered pieces in merge order, and which layer each effective value comes from — the home for how merge-priority conflicts and ties render. Read-only, no VM preflight. |
| `viv config eval [--json]` | Render the fully merged, **evaluated** configuration for the bound manifest — the "what did my layers produce" view. Runs the module merge, so it guards on the hard preflight subset (Nix present); `65` (EX_DATAERR) if evaluation fails. Replaces the retired `viv show --resolved`. |
| `viv status [--json] [-g\|--global]` | Report the operational state of the project's VM — one of `absent`, `built`, `starting`, `running` (with a `stale` flag), `stopping`, `failed` ([`10-vm-lifecycle.md`](10-vm-lifecycle.md)). Project-local by default; `-g`/`--global` (scoped to `status`) enumerates every project in the state registry. Pure read-only; never mutates state and runs no build or preflight. Any reported state — including `failed` — exits `0`; the state is data. Distinct from `doctor` (host/prereq health) and `config` (configuration). |
| `viv doctor [--json] [--strict] [--list] [--online]` | Diagnose the host and project setup from the shared probe catalog: virtualization, tooling, permissions, disk, and config sanity. A pure health checker (`pass`/`warn`/`fail`/`skipped` with sysexit codes); it never renders configuration — that is `viv config`'s job. Offline by default (`--online` adds network checks); `--strict` fails on warnings; `viv start`'s preflight runs the hard subset of this same catalog. Full contract in [`13-doctor-and-health-checks.md`](13-doctor-and-health-checks.md). |

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

## Global flags

A small set of flags is **global** — accepted before or after any subcommand (`viv -v start` and
`viv start -v` are equivalent) and handled uniformly, never redeclared per command
([`../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../decisions/ADR-0026-global-flags-and-config-precedence.md)):

- `-v` / `--verbose` — increase diagnostic verbosity on stderr; stackable (`-vv`, `-vvv` for trace).
  Tunes stderr only; it never adds to or reshapes stdout data.
- `-q` / `--quiet` — suppress non-error progress and status on stderr; it never suppresses errors.
  `-v` and `-q` are mutually exclusive (last one wins).
- `--log-file <path>` / `--log-level <level>` / `--log-format <logfmt|json>` / `--no-log` — control
  the always-on diagnostic **log file** (the machine/debug face). Verbosity (`-v`/`-q`) tunes the
  **stderr** face; these tune the **file** face, independently. Full contract in
  [`16-logging-and-diagnostics.md`](16-logging-and-diagnostics.md).

Machine output is deliberately **not** global: each data command owns its own `--json` flag (one
JSON value on stdout), rather than a global `--format`/`-o`. This keeps every command's output
schema independent and stable for automation and coding-agent consumers.

Cross-cutting settings resolve by a single precedence rule — **flag > environment variable >
default** (there is no user config file for these). `--manifest` / `VIVARIUM_MANIFEST` above obey
it; color follows the env-only chain `NO_COLOR > FORCE_COLOR > isatty` with no `--color` flag
(ADR-0015).

## Output streams

The stream and machine-output rules are the same for every command, specified in
[`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md):

- **stdout carries the result only** — a human table/line for data commands (`images list`,
  `manifest show`, `generations list`, `volume list`, `config`/`config sources`/`config eval`,
  `status`, `doctor`'s report), a `--json` record in machine mode, and
  **nothing** for side-effect commands whose result is a VM state change (`start`, `stop`,
  `destroy`).
- **stderr carries everything else** — progress, status, prompts, warnings, errors. Progress is shown
  only when stderr is a TTY, so `… --json 2>/dev/null | jq` is always clean.
- **A diagnostic log file is written by default** — a third, structured face separate from stdout and
  stderr, invisible during normal use. It is the machine/debug channel, fully specified in
  [`16-logging-and-diagnostics.md`](16-logging-and-diagnostics.md).
- `exec`/`shell` pass guest stdio transparently as specified in
  [`12-exec-and-shell.md`](12-exec-and-shell.md); vivarium progress remains on stderr only so guest
  stdout stays pipeable.
- Color is human-only, honoring `NO_COLOR > FORCE_COLOR > isatty`; JSON and non-TTY output are never
  colored.

## Failure and preflight

- Commands return zero on success and a **specific** non-zero code on failure, from the program-wide
  BSD sysexits taxonomy (never a generic `1`). The complete legend and the per-command exit-code
  matrix are in [`14-exit-codes.md`](14-exit-codes.md); the contract is
  [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md)
  as amended by
  [`../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md`](../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md).
- Commands that need host prerequisites run a **preflight guard** — the hard subset of the shared
  `viv doctor` probe catalog — and refuse **before any side effect**. Each failure reports
  what / where / why / hint plus a stable check id. See
  [`10-vm-lifecycle.md`](10-vm-lifecycle.md) for `viv start`'s preflight.
- `viv exec` and `viv shell` use sysexits for vivarium-origin failures before a guest process starts;
  after the guest command or shell starts, they return its exit status verbatim, with signal deaths
  reported as `128+S`. See [`12-exec-and-shell.md`](12-exec-and-shell.md).
- Diagnostics (`doctor`, `config`, `config sources`, `config eval`, `status`, `generations list`,
  `volume list`) are read-only and never modify project or VM state.

## Config inspection output

The `config` family (ADR-0022) obeys the stream and `--json` rules above; this section fixes each
command's concrete shape.

- **`viv config`** (the binding) — human output lists the bound manifest and the effective
  config/state/data/cache paths, one per line. `--json` emits one binding record:
  `{ "manifest": <name|null>, "source": "flag|env|registry", "paths": { "config", "state", "data",
  "cache" } }`. When no manifest resolves it **fails closed** (`78`, no manifest — see
  [`14-exit-codes.md`](14-exit-codes.md)), never an empty record.

- **`viv config eval`** (the merged, evaluated config) — human output is **TOML-shaped**, mirroring
  the `.vivarium.toml` authoring surface so the merged result reads in the same language it was
  written in:

  ```toml
  [resources]
  mem_mib = 4096
  vcpu    = 4

  [sandbox.egress]
  mode  = "allowlist"
  allow = ["api.crates.io", "github.com"]

  [[mounts]]
  source   = "~/.config/foo"
  target   = "~/.config/foo"
  readonly = true
  ```

  `--json` emits an **enveloped** record — the evaluated config nested under `config`, alongside the
  identity that produced it: `{ "manifest", "image", "pieces", "config": { … } }`. Evaluation
  failure (an irreconcilable merge such as one volume name bound to two mountpoints) is a hard
  error: what / where / why / hint on stderr and `65` (EX_DATAERR), never a partial render.

- **`viv config sources`** (provenance and conflicts) — human output is a **per-key block**: each
  effective key, its value, then its ordered contributors with layer, kind, and priority
  (`mkDefault` / normal / `mkForce`), the winner and shadowed layers marked. Lists show each
  element's originating layer (lists concatenate, see
  [`04-composition-and-determinism.md`](04-composition-and-determinism.md)). An **equal-priority
  tie** — one key set at equal priority by two layers, resolved only by declaration order — is
  flagged with a `[tie]` marker and a fix hint:

  ```text
  sandbox.egress.mode = allowlist
    base-rust      image  mkDefault  "open"       (shadowed)
    net-allowlist  piece  mkForce    "allowlist"  (winner)

  [tie] resources.mem_mib = 4096   (equal priority — resolved by order)
    rust-toolchain  piece  normal  4096  (winner, declared first)
    dev             leaf   normal  8192  (shadowed)
    hint: raise the intended layer with mkForce, or reorder pieces.
  ```

  `--json` emits `{ "manifest", "image", "pieces", "values", "conflicts" }`: `values` maps each key
  to its `effective` value, `winner`, and full `contributors` list; `conflicts` is the
  normally-empty array of equal-priority ties. A tie is **not** an error — exit `0`, with the
  `[tie]` note on stderr so `… --json 2>/dev/null | jq` stays clean and scripts detect ties via
  `conflicts`.

## Library inspection output

The library readers (`images list`, `manifest list`, `manifest show`) obey the same stream and
`--json` rules; this section fixes each command's concrete shape. Like the `config` family, each
emits a **single keyed JSON object** with **no `schema_version`** — per-command stability is the
versioning boundary (see the machine-output rule above), not a versioned envelope. That envelope is
reserved for `doctor`, whose health-check protocol earns it
([`13-doctor-and-health-checks.md`](13-doctor-and-health-checks.md)). Lists are wrapped in a named
key (not a bare top-level array) so future metadata can be added without a breaking re-wrap.

- **`viv images list`** — `--json` emits `{ "images": [ { "name", "path" } ] }`: `name` is the
  kebab-case identifier used in a manifest's `image = "…"`; `path` is the absolute path to the image
  module under the config root's `images/` ([`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)).
  An empty or absent library emits `{ "images": [] }` and exits `0`, never an error.

- **`viv manifest list`** — `--json` emits `{ "manifests": [ { "name", "path", "image", "pieces" } ] }`,
  one row per *defined* manifest (not the bound one — that is `viv config`). `pieces` is the ordered
  piece list; `image` and `pieces` are included so the composition is visible without a follow-up
  `show`. Empty/absent library emits `{ "manifests": [] }` and exits `0`.

- **`viv manifest show <name>`** — `--json` emits the **declared** manifest, mirroring its TOML
  authoring surface ([`03-artifact-model.md`](03-artifact-model.md)) — *not* the evaluated merge,
  which is `config eval`'s job:

  ```json
  {
    "manifest": "rust-web",
    "path": "/home/alice/.config/vivarium/manifests/rust-web.toml",
    "image": "rust",
    "pieces": ["git", "ssh-agent", "direnv", "egress-open"],
    "resources": { "mem_mib": 4096, "vcpu": 4 },
    "egress": { "mode": "open", "allow": [] },
    "extends": null
  }
  ```

  `manifest` is the identity key (as in the `config` family). `resources.*` fields are `null` when
  undeclared. `egress.mode` is `"open"` | `"allowlist"` (declared top-level `[egress]`, per
  [`05-networking-and-egress.md`](05-networking-and-egress.md)); `egress.allow` is the allowlist
  declared at this layer (empty under `open`). `extends` is the optional raw-`.nix` escape hatch,
  `null` when unused. Unknown name fails closed `78`; bad arg arity `64` (see
  [`14-exit-codes.md`](14-exit-codes.md)).

## Status output

`viv status` reports operational VM state (the lifecycle states in
[`10-vm-lifecycle.md`](10-vm-lifecycle.md)); it obeys the same stream and `--json` rules and, like
the readers above, emits a **single keyed JSON object** with **no `schema_version`**. It runs no
build or preflight.

- **`viv status`** (project-local) — human output names the bound manifest and the current state, and
  when running adds the generation, store path, uptime, and declared resources; a **stale** running
  VM is flagged with the remedy (`viv start --rebuild`). `--json` emits one record:

  ```json
  {
    "manifest": "rust-web",
    "state": "running",
    "stale": true,
    "generation": 42,
    "store_path": "/nix/store/…-vivarium",
    "uptime_seconds": 8100,
    "resources": { "mem_mib": 4096, "vcpu": 4 }
  }
  ```

  `manifest` is the identity key (as in the `config` family). `state` is one of `absent`, `built`,
  `starting`, `running`, `stopping`, `failed`. `stale` is meaningful only while `running`. When
  `state` is `failed`, a `reason` field carries the cause (`crashed`, `boot-timeout`, …) and the
  liveness fields (`generation` aside) are `null`. Any reported state — including `failed` — exits
  `0`; the state is *data*, not a command failure. No manifest bound fails closed `78`; a state that
  cannot be confirmed (backend unreachable) is `69` ([`14-exit-codes.md`](14-exit-codes.md)).

- **`viv status -g` / `--global`** — enumerate every project in the state registry
  ([`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)). `-g` is scoped to `status`, not a
  global flag (nothing else enumerates). `--json` wraps the list under a named key, matching
  `images`/`manifests`:

  ```json
  { "projects": [ { "manifest": "rust-web", "project_path": "/home/alice/backend",
                    "state": "running", "stale": false, "generation": 42 } ] }
  ```

  An empty registry emits `{ "projects": [] }` and exits `0`; a registry I/O failure is `74`.
