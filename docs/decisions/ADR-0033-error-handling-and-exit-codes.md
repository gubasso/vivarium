# ADR-0033: Error handling and exit-code realization

## Context and Problem Statement

The exit-code taxonomy is a settled, permanent API — a BSD sysexits subset plus two owned codes, `1` (only `viv doctor --strict`) and `128+S` (guest-signal pass-through) ([`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md), [`ADR-0028-exit-code-taxonomy-and-stability.md`](./ADR-0028-exit-code-taxonomy-and-stability.md)). The clippy lints forbid `panic!` and warn on `unwrap`/`expect`, so errors must be values threaded to one boundary. This ADR fixes the error-type stack and how a typed error becomes a process code.

## Considered Options

- Adopt the `sysexits` crate for the `EX_*` constants and model the two owned codes separately.
- Hand-roll one `ExitKind` enum as the single source of truth for every process code.
- Use only a dynamic error type (`anyhow`/`eyre`) with ad hoc exit-code mapping at call sites.

## Decision Outcome

Chosen option: **`thiserror` + `anyhow`, with a hand-rolled `ExitKind` enum.**

- **`thiserror`** derives typed, per-layer error enums; **`anyhow`** carries context at the application edge. This matches the lints — errors are returned, never `panic!`-ed.
- **One `ExitKind` enum owns the mapping.** Its variants cover the whole spec/14 set: the sysexits subset (`64/65/69/70/74/75/77/78`) _and_ the two codes the sysexits set cannot express — `DoctorStrict` (`1`) and `GuestStatus(u8)` (`0..=255`, `128+S`). The `sysexits` crate is rejected precisely because adopting it for the subset would split exit-code ownership in two: it structurally cannot represent the owned codes, so a single hand-rolled enum keeps one source of truth, matching ADR-0028's append-only guarantee.
- **`main` returns `std::process::ExitCode`** via `From<ExitKind>`; the typed error is rendered (stderr what/where/why/hint or `--json`) and converted once at the boundary. No `std::process::exit`.

## Consequences

- Good: every code lives in one auditable place that mirrors spec/14 exactly; lints are satisfied by construction.
- Good: no dependency whose model conflicts with the owned codes.
- Bad: the `EX_*` constants are maintained locally rather than pulled from a crate.

## Status

Accepted

Amended by [`ADR-0069-redaction-is-by-construction.md`](./ADR-0069-redaction-is-by-construction.md) — adds a secret-carrying newtype to the error/field type stack: `Debug` and `Display` render `[REDACTED]` and it is not serializable, so the boundary renderer above cannot emit a secret even in an error chain. The `ExitKind` mapping is unchanged.

Amended by [`ADR-0068-human-error-presentation-and-diagnostic-ids.md`](./ADR-0068-human-error-presentation-and-diagnostic-ids.md) — the typed error carries a stable diagnostic id alongside its `ExitKind`, and the boundary renderer emits the fixed skeleton (or the JSON failure object on stderr) specified in [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
