# ADR-0015: CLI output and failure contract

## Context and Problem Statement

vivarium's commands must be usable by humans at a terminal and by scripts and agents parsing output,
and they must fail in a diagnosable, machine-matchable way. Without one contract, output drifts —
progress bleeds into stdout and breaks `| jq`, and failures collapse to a generic exit `1` that
carries no signal. We need one rule for streams, machine output, and failure.

## Considered Options

- **Ad-hoc per-command** — each command decides its streams and exit codes.
- **One house contract** — a fixed stdout/stderr split, `--json` for machine output, a stable
  exit-code taxonomy, and a shared preflight pattern.
- **Custom small exit-code table** (e.g. `10/20/30/40`) instead of a standard one.

## Decision Outcome

Chosen option: **one house contract**.

- **Streams.** stdout carries the *result only* — a human table/line for data commands, a `--json`
  record in machine mode, and **nothing** for side-effect commands like `up`/`down` (their result is
  a side effect). stderr carries everything else: progress, status, prompts, warnings, errors.
  Progress is shown only when stderr is a TTY.
- **Machine output.** `--json` emits one structured record on stdout, so `… --json 2>/dev/null | jq`
  is always clean. Color is human-only, honoring `NO_COLOR > FORCE_COLOR > isatty`; never color JSON
  or non-TTY output.
- **Failure.** Adopt **BSD sysexits** (e.g. `69` unavailable, `77` permission, `78` config, plus
  build/launch codes) — never a generic `1`. Every error renders **what / where / why / hint**.
- **Preflight.** One probe catalog, three call sites (`doctor` runs all; each command guards its
  hard subset; setup reuses it); guards refuse **before any side effect**.

## Consequences

- Good: pipeable, agent-friendly output and diagnosable, matchable failures.
- Good: `doctor` and per-command guards cannot drift — one catalog.
- Bad: sysexits codes are coarse; some conditions share a code and lean on the message.

## Status

Accepted

Amended by
[`ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](ADR-0018-lifecycle-verbs-and-teardown-boundary.md) —
the side-effect commands named above are now `start`/`stop`/`destroy` (formerly `up`/`down`); the
contract itself is unchanged.

Amended by
[`ADR-0023-doctor-check-catalog-and-contract.md`](ADR-0023-doctor-check-catalog-and-contract.md) —
the "never a generic `1`" rule gains one narrow exception: `viv doctor --strict` returns `1` when it
promotes a soft warning to a failure. That is a policy signal for CI gates on a read-only command
with no side effect, not an operational failure; all operational failures keep the sysexits
taxonomy.

Applied across [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md)
and [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md).
