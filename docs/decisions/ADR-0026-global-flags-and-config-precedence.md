# ADR-0026: Global flags and configuration precedence

## Context and Problem Statement

ADR-0015 fixed streams, `--json`, color, and sysexits, but left three CLI-surface questions open:
whether verbosity is global, whether machine output stays per-command `--json` or becomes a global
`--format`, and the general precedence between flags, environment variables, and defaults. Four
candidate flags — `--format`, `-v`/`-q`, `--log-format`, `--keep-generated` — needed classifying.

## Considered Options

- Global `--format json|table|nix` (docker/kubectl `-o` style) replacing per-command `--json`.
- No global flags — every flag declared per command.
- Cross-cutting concerns global, data-shaping per-command, with a documented precedence chain.

## Decision Outcome

Chosen option: **cross-cutting global, data-shaping per-command**.

- **Verbosity is global.** `-v`/`--verbose` (stackable `-vv`/`-vvv`, trace ceiling) and `-q`/
  `--quiet` are accepted before or after any subcommand and tune stderr diagnostics only — never
  stdout data; `-v`/`-q` are mutually exclusive (last one wins). Matches git/cargo/nix.
- **Machine output stays per-command `--json`** — one JSON value on stdout. No global `--format`/
  `-o`; the candidate `table`/`nix` values are dropped. The gh/nix per-command `--json` model fits a
  greenfield tool whose machine consumers are automation and coding agents; the docker/kubectl
  multi-render surface is more than vivarium needs.
- **Precedence standard: `flag > environment variable > default`** for every cross-cutting concern
  (there is no user config file yet). Color stays env-only — `NO_COLOR > FORCE_COLOR > isatty`, no
  `--color` flag — reaffirming ADR-0015.
- **`--log-format` is dropped** from the surface here, and later specified by ADR-0031 — an
  active-by-default log file controlled by `--log-format`/`VIV_LOG_FORMAT` and the other `--log-*`
  flags, with no `viv logs` command. **`--keep-generated` is removed** — retention is the
  generations model's job (ADR-0014), not a cross-cutting flag.

## Consequences

- Good: global flags documented once; behavior is predictable across every subcommand; `--json`
  stays stable for agents.
- Good: one precedence rule governs every cross-cutting concern.
- Bad: verbosity as a global flag needs root-parser handling, and `-v`'s exact detail differs per
  command.

## Status

Accepted

Amended by [`ADR-0031-logging-and-observability.md`](ADR-0031-logging-and-observability.md) — the
`--log-format` deferral is discharged: logging is specified as an active-by-default log file with
`--log-*` flags and `VIV_LOG*` env vars.

Amends
[`ADR-0015-cli-output-and-failure-contract.md`](ADR-0015-cli-output-and-failure-contract.md) — adds
the global-flag taxonomy and the `flag > env var > default` precedence standard; makes verbosity/
quiet global; keeps `--json` per-command; reaffirms the env-only color model. Applied in
[`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md); reconciled in
[`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
