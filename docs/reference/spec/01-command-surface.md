# 01 — Command surface

The command-line verbs vivarium exposes. This page is the lookup table for the surface; the intended end-to-end flow is walked in [`../../guides/getting-started.md`](../../guides/getting-started.md), and current implementation state is tracked in [`../implementation-status.md`](../implementation-status.md).

Project-local commands operate on the manifest selected for the invoking directory by the precedence in [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md).

## Verbs

| Command                                                                        | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `viv images list [--json]`                                                     | List the images in the config library (`$XDG_CONFIG_HOME/vivarium/images/`). Read-only enumeration; an empty or absent library lists zero rows and exits `0`, never an error.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `viv manifest list [--json]`                                                   | List the manifests in the config library (`manifests/`). Enumerates all defined manifests, not only the one selected for the current directory — that selection is shown by `viv config`. Empty/absent library lists zero rows and exits `0`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `viv manifest show <name> [--json]`                                            | Show the named manifest's declared image, ordered pieces, and policy knobs, read from the config library. Takes exactly one `<name>`; an unknown name fails closed with a non-zero exit. Distinct from `config eval`, which evaluates the full module merge.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `viv start [--rebuild \| --no-rebuild] [--generation <n>] [--attach] [--json]` | Resolve the selected manifest, build the VM, and boot it with every declared workspace mounted — "ensure built and running." Detached by default (boots and returns); idempotent and non-destructive to a running VM. Full behavior in [`10-vm-lifecycle.md`](./10-vm-lifecycle.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `viv exec [-t\|--tty] [-T\|--no-tty] [--env KEY[=VAL]]... -- <cmd> [args...]`  | Run a command inside the project VM, starting it first if needed; `--` is required and all following args are guest argv. See [`12-exec-and-shell.md`](./12-exec-and-shell.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `viv shell`                                                                    | Open a login-interactive PTY shell inside the project VM, starting it first if needed. See [`12-exec-and-shell.md`](./12-exec-and-shell.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `viv stop [--all] [--force] [-t\|--timeout <secs>] [--json]`                   | Gracefully stop the selected sandbox VM (in-guest agent shutdown, falling back to hard poweroff after `--timeout`, default 10 s; `-1` waits indefinitely), preserving persistent volumes and build generations (N18). `--force` powers off immediately (possible data loss) and conflicts with a nonzero `--timeout` (usage error). `--all` applies the same ladder to every running sandbox and needs no selected manifest. Idempotent: nothing running → no-op, exit `0`. Full behavior in [`10-vm-lifecycle.md`](./10-vm-lifecycle.md).                                                                                                                                                                                                                                                                                                                                                                                                          |
| `viv trim [--to <MiB>] [--json]`                                               | Reclaim memory the project VM is holding but no longer needs, and report what the host got back. Bounded, synchronous, and always explicit — vivarium never reclaims from a running guest on its own (N23). Requires a running VM (`75` otherwise). See [`17-resources-and-capacity.md`](./17-resources-and-capacity.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `viv destroy [-f\|--yes] [--keep-volumes] [--json]`                            | Tear the selected sandbox down: graceful stop, unlink all build generations, remove persistent volumes and runtime state. Prompts on a TTY; non-interactive runs require `--yes`. `--keep-volumes` preserves volumes. Store paths become reclaimable and are freed only by a later `viv gc`. It never touches a workspace or config, and the next `start` rebuilds from the same manifest declaration. Idempotent. See [`10-vm-lifecycle.md`](./10-vm-lifecycle.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `viv update [<input>...] [--json]`                                             | Re-resolve the project's pinned build inputs and rewrite its lockfile ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). With no argument every input moves; with names, only those. This is the only command that moves a lock — `start` never re-resolves, which is what keeps a build reproducible between updates. It reports each input's before and after and does not build; the next `start` produces the generation. A project with no lock yet is not an error: `update` creates one, as does the first build when `update` has not run, and either way the pin is reported ([`../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)). It refuses while a team override lock is in force, because the file it would write is not the file that is read (`78`, [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)).        |
| `viv generations <list\|activate\|rollback\|prune> [--json]`                   | Manage the retained build generations: `list` (number, timestamp, store path), `activate`/`rollback` the current pointer, `prune [--keep <n>] [--older-than <dur>]` under a retention policy, unlinking GC roots. See [`11-generations-and-build-history.md`](./11-generations-and-build-history.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `viv gc`                                                                       | Run the store garbage collector — a global, whole-store sweep that reclaims store paths unreachable from any GC root, vivarium's or not. Project-scoped retention lives in `viv generations prune`. See [`11-generations-and-build-history.md`](./11-generations-and-build-history.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `viv volume <list\|rm\|trim\|prune> [--json]`                                  | Manage the project's persistent volumes: `list` shows the default and named volumes with mountpoints, declaring layer, orphans, and allocated-against-virtual size; `rm <name>` / `rm --all` remove volume data; `trim [<name>]` returns space freed inside a volume to the host image; `prune [-n\|--dry-run] [-f\|--yes]` removes orphans only — never a declared volume, and there is no `--all` — prompting on a TTY and requiring `--yes` non-interactively, with `--dry-run` printing exactly the rows a run would remove. Removal and `prune` refuse while the VM runs (stop first); `trim` requires a running VM; nothing to prune is a no-op, exit `0`. Creation is declarative only — volumes are declared in the manifest or pieces and materialized lazily by `start`. See [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md) and [`17-resources-and-capacity.md`](./17-resources-and-capacity.md). |
| `viv config [--json]`                                                          | Inspection namespace for the selected manifest's configuration (ADR-0022). With no subcommand, shows the manifest, selection source, and effective config/state/data/cache paths. Read-only, no VM preflight.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `viv config sources [--json]`                                                  | Provenance view: the declaring manifest and its ordered pieces in merge order, and which layer each effective value comes from — the home for how merge-priority conflicts and content defects render. Read-only, but it reads the module system's definition list, so like `config eval` it guards on the hard preflight subset (Nix present). It renders a defect and still exits `0` ([`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)).                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `viv config eval [--json]`                                                     | Render the fully merged, evaluated configuration for the selected manifest — the "what did my layers produce" view. Runs the module merge, so it guards on the hard preflight subset (Nix present); a recognized content defect is `65` (EX_DATAERR) and any other evaluation fault is `70`, the boundary drawn in [`14-exit-codes.md`](./14-exit-codes.md). Replaces the retired `viv show --resolved`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `viv status [--json] [-g\|--global]`                                           | Report the operational state of the selected sandbox VM — one of `absent`, `built`, `starting`, `running` (with a `stale` flag), `stopping`, `failed` ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)). Project-local by default; `-g`/`--global` enumerates the sandbox fleet. Pure read-only; never mutates state and runs no build or preflight. Any reported state — including `failed` — exits `0`; the state is data. Distinct from `doctor` (host/prereq health) and `config` (configuration).                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `viv doctor [--json] [--strict] [--list] [--online]`                           | Diagnose the host and project setup from the shared probe catalog: virtualization, tooling, permissions, disk, and config sanity. A pure health checker (`pass`/`warn`/`fail`/`skipped` with sysexit codes); it never renders configuration — that is `viv config`'s job. Offline by default (`--online` adds network checks); `--strict` fails on warnings; `viv start`'s preflight runs the hard subset of this same catalog. Full contract in [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md).                                                                                                                                                                                                                                                                                                                                                                                                                              |

## Selection and overrides

- `--manifest <name>` is the highest-precedence single-invocation override; `VIVARIUM_MANIFEST` is the next. Neither is persisted. The third rung derives the unique owner from explicit `[[workspaces]]` declarations in the manifest library (see [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)).
- Where no manifest owns the invoking directory, or more than one claims it, commands that require one fail closed at `78` without prompting. The hint names the exact `[[workspaces]]` block to add for no owner; ambiguity names both manifests.

## Global flags

A small set of flags is global — accepted before or after any subcommand (`viv -v start` and `viv start -v` are equivalent) and handled uniformly, never redeclared per command ([`../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../decisions/ADR-0026-global-flags-and-config-precedence.md)):

- `-h` / `--help` — print usage on stdout and exit `0`. Anywhere before the first `--` it wins over every other token, including a malformed rest — `viv start --bogus --help` is a user asking what `start` takes, not a usage error — and `viv <verb> --help` answers for that verb.
- `--version` — print `viv <version>` on stdout and exit `0`; `--help` wins when both are present.
- `-v` / `--verbose` — increase diagnostic verbosity on stderr; stackable (`-vv`, `-vvv` for trace). Tunes stderr only; it never adds to or reshapes stdout data.
- `-q` / `--quiet` — suppress non-error progress and status on stderr; it never suppresses errors. `-v` and `-q` are mutually exclusive (last one wins).
- `--log-file <path>` / `--log-level <level>` / `--log-format <logfmt|json>` / `--no-log` — control the always-on diagnostic log file (the machine/debug face). Verbosity (`-v`/`-q`) tunes the stderr face; these tune the file face, independently. Full contract in [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md).
- `--no-console-log` — do not capture the running VM's serial console to `console.log`. A separate channel from all of the above, and unaffected by them ([`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)).

Machine output is deliberately not global: each data command owns its own `--json` flag (one JSON value on stdout), rather than a global `--format`/`-o`. This keeps every command's output schema independent and stable for automation and coding-agent consumers.

Cross-cutting settings resolve by a single precedence rule — flag > environment variable > default (there is no user config file for these). `--manifest` / `VIVARIUM_MANIFEST` above obey it; color follows the env-only chain `NO_COLOR > FORCE_COLOR > isatty` with no `--color` flag (ADR-0015).

## Output streams

The stream and machine-output rules are the same for every command, specified in [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md):

- stdout carries the result only — a human table/line for data commands (`images list`, `manifest show`, `generations list`, `volume list`, `config`/`config sources`/`config eval`, `status`, `doctor`'s report), a `--json` record in machine mode, and nothing for side-effect commands whose result is a VM state change (`start`, `stop`, `destroy`). `trim`, `volume trim`, and `volume prune` are the exception that proves the rule: they act, but what a user runs them for is the measurement they return, so they print it (shape below).
- stderr carries everything else — progress, status, prompts, warnings, errors. Progress is shown only when stderr is a TTY, so `… --json 2>/dev/null | jq` is always clean. Progress is also suppressed under `--json` regardless of TTY: a caller that selected machine mode is a machine, and every non-result stderr byte it receives is noise it must filter. This includes the `--json` failure envelope: a failure has no result, so its machine-readable form is one JSON object on stderr, never on stdout. Its shape and the human skeleton beside it are in [`14-exit-codes.md`](./14-exit-codes.md).
- A diagnostic log file is written by default — a third, structured face separate from stdout and stderr, invisible during normal use. It is the machine/debug channel, fully specified in [`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md).
- `exec`/`shell` pass guest stdio transparently as specified in [`12-exec-and-shell.md`](./12-exec-and-shell.md); vivarium progress remains on stderr only so guest stdout stays pipeable.
- Color is human-only, honoring `NO_COLOR > FORCE_COLOR > isatty`; JSON and non-TTY output are never colored.

## Failure and preflight

- Commands return zero on success and a specific non-zero code on failure, from the program-wide BSD sysexits taxonomy (never a generic `1`). The complete legend and the per-command exit-code matrix are in [`14-exit-codes.md`](./14-exit-codes.md); the contract is [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md) as amended by [`../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md`](../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md).
- Commands that need host prerequisites run a preflight guard — the hard subset of the shared `viv doctor` probe catalog — and refuse before any side effect. Each failure reports what / where / why / hint plus a stable check id — which is also its diagnostic id, in the one id space fixed by [`14-exit-codes.md`](./14-exit-codes.md). See [`10-vm-lifecycle.md`](./10-vm-lifecycle.md) for `viv start`'s preflight.
- `viv exec` and `viv shell` use sysexits for vivarium-origin failures before a guest process starts; after the guest command or shell starts, they return its exit status verbatim, with signal deaths reported as `128+S`. See [`12-exec-and-shell.md`](./12-exec-and-shell.md).
- Diagnostics (`doctor`, `config`, `config sources`, `config eval`, `status`, `generations list`, `volume list`) are read-only and never modify project or VM state.

## Config inspection output

The `config` family (ADR-0022) obeys the stream and `--json` rules above; this section fixes each command's concrete shape.

- `viv config` (the selection) — human output lists the selected manifest and the effective paths, one per line. `--json` emits one selection record: `{ "manifest": <name|null>, "source": "flag|env|derived", "paths": { "config", "state", "data",
  "cache", "flake", "lock" } }`. The first four are the XDG roots; `flake` is this project's generated flake directory and `lock` is the lockfile in force — the manifest's team override lock when one is present, otherwise the per-target lock under the data root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). Both are reported whether or not they exist yet, because their value to a reader is knowing where to look; this is how the generated flake is inspected, and why no separate flag retains it. When no manifest resolves it fails closed (`78`, no manifest — see [`14-exit-codes.md`](./14-exit-codes.md)), never an empty record.

- `viv config eval` (the merged, evaluated config) — human output is TOML-shaped, mirroring the manifest's own authoring surface so the merged result reads in the same language it was written in:

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

  `--json` emits an enveloped record — the evaluated config nested under `config`, alongside the manifest and layers that produced it: `{ "manifest", "image", "pieces", "config": { … } }`. A content defect — an irreconcilable merge such as one volume name bound to two mountpoints, an equal-priority scalar tie, or a literal personal path in a shared image or piece (N11) — is a hard error: what / where / why / hint on stderr and `65` (EX_DATAERR), never a partial render. `viv config sources` renders the same defect without failing, so it stays usable at exactly this moment ([`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)).

- `viv config sources` (provenance and defects) — human output is a per-key block: each effective key, its value, then its ordered contributors with layer, kind, and priority (`mkDefault` / normal / `mkForce`), the winner and shadowed layers marked. Lists show each element's originating layer (lists concatenate, see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). Contributors are read from the module system's definition list rather than from the merged value, which is what lets this view still render when the merge itself would fail. An equal-priority tie — one key with two surviving definitions at the same priority — is flagged with a `[tie]` marker and a fix hint:

  ```text
  sandbox.egress.mode = allowlist
    base-rust      image  mkDefault  "open"       (shadowed)
    net-allowlist  piece  mkForce    "allowlist"  (winner)

  [tie] resources.mem_mib   (equal priority — evaluation will fail)
    rust-toolchain  piece  normal  4096
    cache-heavy     piece  normal  8192
    hint: a shared piece should propose with mkDefault so your manifest can
          decide; failing that, drop one piece or override through extends.
  ```

  `--json` emits `{ "manifest", "image", "pieces", "values", "conflicts" }`. `values` maps each key to its `effective` value, `winner`, and full `contributors` list. A key carrying a defect keeps its full `contributors` but reports `effective` and `winner` as `null`: the merge threw and produced no value, and naming a winner anyway would resurrect the declaration-order tiebreak ADR-0042 removed. `conflicts` is the normally-empty array of content defects — equal-priority ties, N11 literal-path violations, N24 session-directory mount sources decidable from text, and non-portable variables in a shared layer's mount sources ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)) — each entry `{ "kind": "tie" | "literal-path" | "session-path" | "non-portable-variable", "key": <option path>, "layers": [ <declaring layer names> ] }`, so a script can branch on the defect class without parsing prose. `config sources` reports them and exits `0`: they are data, not this command's own failure, and this is the command a user reaches for after `config eval` or `start` returned `65` ([`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)). The `[tie]` note goes to stderr so `… --json 2>/dev/null | jq` stays clean, and scripts detect defects via `conflicts`.

## Library inspection output

The library readers (`images list`, `manifest list`, `manifest show`) obey the same stream and `--json` rules; this section fixes each command's concrete shape. Like the `config` family, each emits a single keyed JSON object with no `schema_version` — per-command stability is the versioning boundary (see the machine-output rule above), not a versioned envelope. That envelope is reserved for `doctor`, whose health-check protocol earns it ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)). Lists are wrapped in a named key (not a bare top-level array) so future metadata can be added without a breaking re-wrap.

- `viv images list` — `--json` emits `{ "images": [ { "name", "path" } ] }`: `name` is the kebab-case identifier used in a manifest's `image = "…"`; `path` is the absolute path to the image module under the config root's `images/` ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). An empty or absent library emits `{ "images": [] }` and exits `0`, never an error.

- `viv manifest list` — `--json` emits `{ "manifests": [ { "name", "path", "image", "pieces" } ] }`, one row per defined manifest (not only the selected one — that is `viv config`). `pieces` is the ordered piece list; `image` and `pieces` are included so the composition is visible without a follow-up `show`. Empty/absent library emits `{ "manifests": [] }` and exits `0`.

- `viv manifest show <name>` — `--json` emits the declared manifest, mirroring its TOML authoring surface ([`03-artifact-model.md`](./03-artifact-model.md)) — not the evaluated merge, which is `config eval`'s job:

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

  `manifest` is the sandbox key (as in the `config` family). `resources.*` fields are `null` when undeclared — meaning not yet resolved, not unset: an undeclared knob takes a host-derived value at launch under the auto-sizing policy in [`17-resources-and-capacity.md`](./17-resources-and-capacity.md), which is why the declared readers (`manifest show`, `config eval`, `config sources`) can show `null` while `status` never does for a running VM. Declared or resolved, the value is a ceiling and not a reservation (N22). `egress.mode` is `"open"` | `"allowlist"` (declared top-level `[egress]`, per [`05-networking-and-egress.md`](./05-networking-and-egress.md)); `egress.allow` is the allowlist declared at this layer (empty under `open`). `extends` is the optional raw-`.nix` escape hatch, `null` when unused. Unknown name fails closed `78`; bad arg arity `64` (see [`14-exit-codes.md`](./14-exit-codes.md)).

## Project-state inspection output

`viv volume list` and `viv generations list` read the selected sandbox's own state rather than the config library, so they get their own section — but they follow the same convention as the readers above: a single keyed JSON object, no `schema_version`, the list under a named key. Both are read-only and both need a selected manifest, failing closed with `78` when none resolves ([`14-exit-codes.md`](./14-exit-codes.md)).

- `viv volume list` — human output is a table of the default and named volumes. `--json` emits `{ "volumes": [ { "name", "mount", "declared_by", "orphan", "allocated_bytes", "virtual_bytes" } ] }`. `name` is the volume's identifier, `default` for the reserved home volume. `mount` is its guest path. `declared_by` names the layer that declared it — the manifest or a piece — and is `null` for the two volumes that exist without declaration, the default home volume and the guest store's ([`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md)). `orphan` is `true` for an image on disk that no current layer declares. `allocated_bytes` is what the sparse image actually occupies and `virtual_bytes` its declared ceiling; the two differ by design (N22, [`17-resources-and-capacity.md`](./17-resources-and-capacity.md)). A project whose volumes have never been created still lists its declared volumes with `allocated_bytes` of `0`.

- `viv generations list` — `--json` emits `{ "generations": [ { "number", "current", "store_path", "manifest", "lock_digest", "backend", "built_at" } ] }`, one row per retained generation, oldest first. The fields mirror the per-generation metadata recorded under the state root ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)); `current` is `true` for exactly the generation `current` points at, and `built_at` is an RFC 3339 timestamp. `lock_digest` digests the whole lockfile the generation was built against — retained beside its metadata — rather than naming one input's revision, because a revision does not reproduce an evaluation. A project that has never been built emits `{ "generations": [] }` and exits `0`.

## Reclamation output

`viv trim` and `viv volume trim` mutate, but their result is a measurement rather than a state change ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)), so they print it. Both follow the conventions above: a single keyed JSON object, no `schema_version`, `_bytes` on every measured quantity, and a before/after pair so a consumer never has to trust a delta it cannot check.

- `viv trim [--to <MiB>]` — human output is one line naming what the host got back. `--json` emits one record:

  ```json
  {
    "manifest": "rust-web",
    "target_mib": 3072,
    "mem_used_before_bytes": 6442450944,
    "mem_used_after_bytes": 3489660928,
    "reclaimed_bytes": 2952790016
  }
  ```

  `manifest` is the sandbox key (as in the `config` family). `target_mib` is the figure the guest was asked to reach — the `--to` value, or the working-set-plus-headroom target vivarium derives when `--to` is absent ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)) — and is never `null` for a completed run. `mem_used_before_bytes` and `mem_used_after_bytes` are the VM's scope memory, the same measurement `status` reports as `runtime.mem_used_bytes`, read immediately before and after the operation so the two commands are directly comparable. `reclaimed_bytes` is their difference floored at `0`: a guest that grew during the operation reports `0` rather than a negative number, because the command's promise is what the host got back, not a signed account. A trim that reclaims nothing is a success and exits `0` — it is a fact about the guest, not a failure. A stopped VM is `75` (start first); a running VM whose agent or backend is unreachable is `69` ([`14-exit-codes.md`](./14-exit-codes.md)).

- `viv volume trim [<name>]` — the disk counterpart, carrying the same before/after pair with the rows under a named key:

  ```json
  {
    "volumes": [
      {
        "name": "default",
        "allocated_before_bytes": 12884901888,
        "allocated_after_bytes": 4509715660,
        "reclaimed_bytes": 8375186228
      }
    ],
    "reclaimed_bytes": 8375186228
  }
  ```

  With no `<name>` every volume is trimmed and every row appears; with a `<name>` the list holds exactly one row. `allocated_before_bytes`/`allocated_after_bytes` are the image's allocated size, the same measurement `volume list` reports as `allocated_bytes`, so the two are joinable; the top-level `reclaimed_bytes` is the sum of the rows, so the common question needs no client-side arithmetic. A project whose volumes have never been materialized emits `{ "volumes": [], "reclaimed_bytes": 0 }` and exits `0`.

- `viv volume prune` — removes whole orphan images rather than freeing space inside retained ones, so it reports what each removal returned rather than a before/after pair:

  ```json
  {
    "volumes": [
      {
        "name": "old-cache",
        "mount": "/var/cache/build",
        "allocated_bytes": 3221225472
      }
    ],
    "reclaimed_bytes": 3221225472
  }
  ```

  `allocated_bytes` is the removed image's allocated size, the same measurement `volume list` reports, so a `list` taken beforehand predicts this record exactly. `mount` is the guest path the volume used to occupy, carried from the state that outlived the declaration, because a name alone rarely identifies which volume a user is about to lose. With nothing to prune the record is `{ "volumes": [], "reclaimed_bytes": 0 }` at exit `0`. `--dry-run` emits this same record with nothing removed, so a consumer that plans against it never has to model two shapes — and the human confirmation prompt renders those same rows, so what a user approves and what a script reads are one thing.

## Update output

`viv update` mutates one file — the project's lockfile — and like the reclamation commands its result is the measurement, so it prints it. Same conventions: one keyed JSON object, no `schema_version`, a before/after pair so nothing has to be inferred from a delta.

```json
{
  "manifest": "rust-web",
  "lock": "/home/alice/.local/share/vivarium/projects/…/default/flake.lock",
  "inputs": [
    { "name": "nixpkgs", "before": "a1b2c3d", "after": "e4f5a6b", "changed": true },
    { "name": "microvm", "before": "9c8d7e6", "after": "9c8d7e6", "changed": false }
  ]
}
```

`lock` is the file written, always the per-target lock under the data root. `viv update` never writes a team's override lock, and it does not quietly write the per-target lock the override shadows either: while an override is in force the command refuses, naming both files and exiting `78` (N13, [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`14-exit-codes.md`](./14-exit-codes.md)). That is a different outcome from the one below, and the two must not be read together: an update that moves nothing is a success and exits `0` — every `changed` false is a fact about upstream, not a failure — whereas a refused update produced no JSON at all.

Each row reports one input; `before` is `null` whenever the row's pin is newly created, which covers both the run that creates the lock and an input the existing lock did not yet carry. Either is a success, not an error. Naming inputs restricts the rows to those inputs; an unknown name is `64`.

An `<input>` is one of vivarium's baseline input names — `nixpkgs` and `microvm`, the rows above — or the identifier a shared image or piece declares in its `inputs.toml` ([`03-artifact-model.md`](./03-artifact-model.md)). The two share one namespace, which is why an artifact may not declare a baseline name; the string a user passes here is the same string the artifact wrote and the same `name` this record reports ([`../../decisions/ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md`](../../decisions/ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md)). Updating does not build: the pin has moved and the next `start` produces the generation that records it ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).

## Status output

`viv status` reports operational VM state (the lifecycle states in [`10-vm-lifecycle.md`](./10-vm-lifecycle.md)); it obeys the same stream and `--json` rules and, like the readers above, emits a single keyed JSON object with no `schema_version`. It runs no build or preflight.

- `viv status` (workspace-local) — human output names the selected manifest and the current state, and when running adds the generation, store path, uptime, and the resource ceilings beside what is actually being used ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)); a stale running VM is flagged with the remedy (`viv start --rebuild`). `--json` emits one record:

  ```json
  {
    "manifest": "rust-web",
    "state": "running",
    "stale": true,
    "generation": 42,
    "store_path": "/nix/store/…-vivarium",
    "uptime_seconds": 8100,
    "resources": { "mem_mib": 8192, "vcpu": 8 },
    "runtime": {
      "mem_used_bytes": 2254857830,
      "disk_allocated_bytes": 4509715660,
      "disk_virtual_bytes": 34359738368,
      "sessions": 3,
      "pressure_some_avg60": 0.1
    }
  }
  ```

  `manifest` is the sandbox key (as in the `config` family). `state` is one of `absent`, `built`, `starting`, `running`, `stopping`, `failed`. `stale` is meaningful only while `running`. When `state` is `failed`, a `reason` field carries the cause (`crashed`, `boot-timeout`, …) and the liveness fields (`generation` aside) are `null`. The reason set is open and grows as vivarium learns to tell one cause from another, so a consumer MUST treat an unrecognized reason as an unclassified failure rather than an error, and MUST NOT assume the named values are exhaustive. Any reported state — including `failed` — exits `0`; the state is data, not a command failure. No manifest selected fails closed `78`; a state that cannot be confirmed (backend unreachable) is `69` ([`14-exit-codes.md`](./14-exit-codes.md)).

  `resources` and `runtime` are deliberately separate objects: `resources` is what was declared or resolved — the ceiling — and `runtime` is what is measured right now. A consumer must never have to guess which it is holding. `resources` fields carry the values in force for this VM, so they are never `null` while it runs (see the resolution rule below). `runtime` is present only while `running`/`stopping`, except `disk_*`, which survive a stop because volumes do (N18); `pressure_some_avg60` may be omitted where the host does not expose it.

- `viv status -g` / `--global` — enumerate every manifest-keyed sandbox represented by retained state ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). `-g` is scoped to `status`, not a global flag — it is the only flag that reports across sandboxes; the one other fleet form, `viv stop --all`, acts rather than enumerates and keeps its own spelling. `--json` wraps the list under a named key, matching `images`/`manifests`, and carries the same `resources` / `runtime` split per row:

  ```json
  { "projects": [ { "manifest": "rust-web", "project_path": "/home/alice/backend",
                    "state": "running", "stale": false, "path_missing": false, "generation": 42,
                    "resources": { "mem_mib": 8192, "vcpu": 8 },
                    "runtime": { "mem_used_bytes": 2254857830, "sessions": 3 } } ],
    "host": { "mem_available_bytes": 10307921510, "mem_total_bytes": 33506172928,
              "pressure_some_avg60": 0.4 } }
  ```

  An empty fleet emits `{ "projects": [], "host": { … } }` and exits `0`; a state I/O failure is `74` and malformed state is `78` ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). `host` is always present — it is what lets the enumeration answer "am I overcommitted?" without a second command.

  `path_missing` is `true` when a declared workspace directory no longer exists; such a row also reports `state: "absent"`. The two flags are unrelated and easy to confuse: `stale` means a running VM is behind its build, `path_missing` means a declared tree is gone.

  A missing workspace is reported, never removed — enumerating is read-only. The human face warns on stderr (so `viv status -g --json | jq` stays clean), names each affected declaration rather than a count, and says why nothing was removed:

  ```console
  warning: 1 declared workspace directory no longer exists:
             rust-web  →  /home/alice/backend
           It may just be on an unmounted filesystem, so vivarium will not
           remove or rewrite the manifest on its own.
  ```

  Naming the declaration matters: a bare count hides which manifest owns the path, and a user not told about the unmount case reads the warning as timidity. The surviving report-rather-than-reap rationale comes from [`../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md`](../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md).
