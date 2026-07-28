# ADR-0032: CLI dependency baseline

## Context and Problem Statement

The `viv` crate has an empty dependency set, but the command surface, manifest model, and async
process/socket work all need foundational libraries. Choosing them once, up front, keeps the stack
coherent and auditable instead of accreting ad hoc. This ADR fixes the parsing, serialization, and
async baseline; the error model and logging stack are decided separately (ADR-0033, ADR-0034).

## Considered Options

- **Arg parser:** `clap` v4 (derive + builder) vs a lean parser (`lexopt`/`bpaf`) vs hand-rolled.
- **TOML:** serde-native `toml` vs format-preserving `toml_edit`.
- **Async:** `tokio` with a trimmed feature set vs a sync design vs `async-std`.

## Decision Outcome

Chosen baseline: **`clap` v4, `serde` + `serde_json` + `toml`, `schemars`, and `tokio`.**

- **`clap` v4** — the command surface needs global flags accepted before *and* after a subcommand
  ([`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md),
  [`ADR-0026-global-flags-and-config-precedence.md`](ADR-0026-global-flags-and-config-precedence.md)),
  deeply nested subcommands, and a caller-owned exit code. `clap`'s `global(true)` and fallible
  `try_parse` meet all three; a lean parser would push nested-subcommand dispatch and help generation
  back onto us. Considered `lexopt`/`bpaf`; rejected on that cost.
- **`serde` + `serde_json` + `toml`** — every data command emits `--json`; manifests are TOML the
  tool only *reads*, never rewrites, so serde-native `toml` is enough and `toml_edit`'s
  format preservation is unwarranted.
- **`schemars`** — realizes the generate-from-types decision
  ([`ADR-0012-generate-config-examples-from-types.md`](ADR-0012-generate-config-examples-from-types.md)).
- **`tokio`** (`rt-multi-thread, macros, process, net, io-util, time, sync, signal`) — `tokio::process`
  supervises `nix build`/hypervisor launches; `net` carries the control socket. The guest control-socket
  transport crate (vsock-class) is deferred to the wire-protocol decision
  ([`ADR-0016-guest-control-transport-and-exec-contract.md`](ADR-0016-guest-control-transport-and-exec-contract.md)).

## Consequences

- Good: a coherent, widely-supported baseline that satisfies the settled CLI and manifest contracts.
- Bad: `clap` and `tokio` add compile time and binary weight versus a minimal parser and sync design.
- Dependencies are added only via `cargo add`/`cargo remove`, never by hand-editing `Cargo.toml`; the
  manifest stays empty until the first module imports a crate.

## Status

Accepted
