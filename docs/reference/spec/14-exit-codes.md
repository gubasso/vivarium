# 14 — Exit codes

The single source of truth for what every `viv` command returns. The governing decisions are
[`../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md`](../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md)
(program-wide categories, the central matrix, append-only stability) and
[`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md)
(the BSD sysexits basis, streams, `--json`). Command-specific behavior is elaborated by
[`12-exec-and-shell.md`](12-exec-and-shell.md) (the guest pass-through boundary) and
[`13-doctor-and-health-checks.md`](13-doctor-and-health-checks.md) (the check catalog and `--strict`).

A code names the **kind** of failure, not the command that raised it: `78` means "config error"
whether it comes from `start`, `config`, or `exec`. Commands map their failure conditions onto this
fixed set; none mints a private code space. The code carries the **category** (for machine
branching); stderr's what / where / why / hint carries the **instance** (the specific file, name, or
step). That split is what keeps the set small — a distinct code exists only where a consumer would
branch on it.

## Legend

| Code | Name | Meaning |
| ---- | ---- | ------- |
| `0` | success | Success, including an idempotent no-op (already running, nothing to remove). |
| `64` | `EX_USAGE` | Bad invocation: unknown flag, wrong arg arity, mutually-exclusive flags, missing `--`, `-t` when stdin is not a terminal. |
| `65` | `EX_DATAERR` | The evaluated/merged configuration is irreconcilable (e.g. one volume name bound to two mountpoints). |
| `69` | `EX_UNAVAILABLE` | A required backend, VM, agent, or store is unavailable: backend missing, running VM/agent unreachable, boot timeout, store sweep failure. |
| `70` | `EX_SOFTWARE` | Nix evaluation/build fault, or an internal error vivarium itself is responsible for. |
| `74` | `EX_IOERR` | I/O failure on a channel vivarium owns: control socket, the state registry, volume/state removal. |
| `75` | `EX_TEMPFAIL` | Transient, retryable: a startup lock/race, or a precondition that clears on retry (a volume still in use by a running VM). |
| `77` | `EX_NOPERM` | Host permission failure: `/dev/kvm`, a filesystem path, or an agent-reported permission denial before the guest process starts. |
| `78` | `EX_CONFIG` | No manifest resolves for a command that needs one, an unknown/invalid manifest, or incompatible generation metadata. |

Two codes sit outside the sysexits set, each with a single owner:

- **`1`** — the *only* sanctioned bare `1`: `viv doctor --strict` promoting a soft `warn` to a
  failure. It is a policy signal on a read-only command, not an operational failure
  ([`13-doctor-and-health-checks.md`](13-doctor-and-health-checks.md), ADR-0023). Every operational
  failure uses the sysexits categories above.
- **`128+S`** — a guest process killed by signal `S`, surfaced only by the `exec`/`shell`
  pass-through below.

## Guest pass-through

`viv exec` and `viv shell` change category at the guest-process-start boundary
([`12-exec-and-shell.md`](12-exec-and-shell.md)):

- **Before** the guest process starts, vivarium-origin failures use the sysexits categories above.
- **After** it starts, vivarium returns the guest status **verbatim** for `0..255`; a guest killed
  by signal `S` yields `128+S`. A guest may itself exit `69` — that is still the guest's result,
  because the boundary is guest-process start.

`127` (command not found) and `126` (not executable) are **reserved** for a future refinement of the
before-start not-found/not-executable cases; v1 uses the sysexits categories and does not emit them.

## Stability

The codes above are a **permanent API**:

- **Never reassigned.** A code's meaning does not change between releases.
- **Append-only.** New categories take fresh, previously-unused numbers; existing ones are never
  repurposed.
- **Branch on categories.** Consumers should test `0` vs non-zero, or a documented category — never
  an undocumented number, and never assume no new category can appear later.

## Command matrix

Every command returns `0` on success. The columns a command can also return on failure:

| Command | `64` | `65` | `69` | `70` | `74` | `75` | `77` | `78` | Notes |
| ------- | ---- | ---- | ---- | ---- | ---- | ---- | ---- | ---- | ----- |
| `init` | ✓ |  |  |  | ✓ |  | ✓ | ✓ | `--write` registry I/O (`74`) / permission (`77`); unknown `--manifest` → `78` |
| `images list` |  |  |  |  |  |  |  |  | read-only; empty/absent library is `0`, never an error |
| `manifest list` |  |  |  |  |  |  |  |  | read-only; empty/absent library is `0`, never an error |
| `manifest show` | ✓ |  |  |  |  |  |  | ✓ | unknown name → `78`; bad arg arity → `64` |
| `start` | ✓ |  | ✓ | ✓ |  | ✓ | ✓ | ✓ | preflight → `69`/`77`/`78`; Nix build → `70`; boot timeout → `69`; lock race → `75` |
| `exec` / `shell` | ✓ |  | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | before guest start only; after start → guest status verbatim (`0..255`, `128+S`) |
| `stop` | ✓ |  | ✓ |  |  |  |  |  | `--force` with a nonzero `--timeout` → `64`; state unconfirmable → `69` |
| `destroy` | ✓ |  | ✓ |  | ✓ |  | ✓ |  | non-interactive without `--yes` → `64`; teardown I/O → `74` / permission → `77` |
| `generations list` |  |  |  |  |  |  |  | ✓ | read-only; no manifest bound → `78` |
| `generations activate` / `rollback` | ✓ |  |  |  | ✓ |  |  | ✓ | unknown generation → `64`; profile-switch I/O → `74`; no manifest → `78` |
| `generations prune` | ✓ |  |  |  | ✓ |  |  | ✓ | bad `--keep` / `--older-than` → `64` |
| `gc` |  |  | ✓ | ✓ |  |  | ✓ |  | store sweep failure → `69`/`70`; permission → `77`; global, needs no manifest |
| `volume list` |  |  |  |  |  |  |  | ✓ | read-only; no manifest bound → `78` |
| `volume rm` | ✓ |  |  |  | ✓ | ✓ | ✓ | ✓ | VM running → `75` (stop first); removal I/O → `74` / permission → `77`; unknown name → `78` |
| `config` |  |  |  |  |  |  |  | ✓ | no manifest resolves → `78` (fails closed) |
| `config sources` |  |  |  | ✓ |  |  |  | ✓ | provenance read fault → `70`; no manifest → `78` |
| `config eval` |  | ✓ | ✓ | ✓ |  |  | ✓ | ✓ | irreconcilable merge → `65`; Nix preflight → `69`/`77`; eval fault → `70`; no manifest → `78` |
| `status` | ✓ |  | ✓ |  |  |  |  | ✓ | any reported state (incl. `failed`) is `0` — state is data; no manifest bound → `78`; state unconfirmable → `69`; bad arity → `64` |
| `status -g` | ✓ |  | ✓ |  | ✓ |  |  |  | enumerate the project registry; empty registry is `0`; registry I/O → `74`; unreadable/corrupt → `69`; bad arity → `64` |
| `doctor` |  |  | ✓ | ✓ |  |  | ✓ | ✓ | first failing hard check decides (`69`/`77`/`78`); internal fault → `70`; `--strict` warn → `1` |

Read-only diagnostics (`doctor`, `config`, `config sources`, `config eval`, `status`,
`generations list`, `volume list`, `images list`, `manifest list/show`) never mutate project or VM
state, whatever they return.
