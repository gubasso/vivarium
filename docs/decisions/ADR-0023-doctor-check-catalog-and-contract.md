# ADR-0023: `viv doctor` check catalog and health-report contract

## Context and Problem Statement

ADR-0022 fixed `viv doctor`'s boundary — a pure health checker that never renders config — but not its contract: catalog, classification, machine output, exit codes. One probe set must serve both human diagnosis and `viv start`'s preflight guard (ADR-0015) without drifting.

## Considered Options

- Flat pass/fail list, generic exit `1`, no machine output.
- Full checker contract: stable-id catalog, hard/soft severity, `skipped` status, offline default with `--online`, `--strict`, `--list`, sysexit-coded failures.
- The same, plus an `--ignore <id>` escape hatch to waive hard checks per invocation.

## Decision Outcome

Chosen option: **full checker contract, without `--ignore`**.

- Every probe has a stable kebab-case id, a category, a scope (host / project / network), and a severity: **hard** — the preflight subset; failing gates `viv start` — or **soft** — advisory. Severity is the only lever: a check that could legitimately be waived is soft by definition, so hard checks are never ignorable and preflight remains an unconditional guarantee.
- Results are `pass | warn | fail | skipped`. Project-scope checks are skipped (with a reason) when no manifest is bound; network-scope checks are skipped unless `--online`. Skips never affect the exit code.
- Exit is `0` when no hard check fails; otherwise the first failing hard check's sysexit code, in catalog order. `--strict` promotes any warn to exit `1` — the warnings-as-errors convention of lint tooling — a narrow, documented exception to ADR-0015's "never a generic `1`": policy promotion on a read-only command, not an operational failure.
- `--json` emits one enveloped record; a check's `doc_url` is optional and omitted (never null) until its page exists. Human output uses bracketed word markers, never unicode glyphs.

## Consequences

- Good: one catalog serves `doctor` and every command guard — no drift.
- Good: severity discipline replaces waiver flags; scripts branch on the summary, not prose.
- Bad: `--strict`'s exit `1` is a carried exception to the sysexits rule.

## Status

Accepted

Amended by **ADR-0049** — adds the membership criterion this contract never had. It fixes the shape of a catalog entry, not the catalog's membership; ADR-0049 supplies the rule that decides membership: a probe exists for a host condition, and anything the lockfile determines is an evaluation-time assertion instead. Stable ids remain stable, but the catalog is not append-only the way the exit-code taxonomy is.

Amends [`ADR-0015-cli-output-and-failure-contract.md`](./ADR-0015-cli-output-and-failure-contract.md) — adds the `--strict` exit-`1` exception. Specified in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md) and [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md).
