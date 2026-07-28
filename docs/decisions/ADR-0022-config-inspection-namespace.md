# ADR-0022: Config inspection under `viv config`, retire `viv show`

## Context and Problem Statement

Three read-only verbs overlapped: `viv show --resolved` (render the merged, evaluated config), `viv config` (bound manifest + effective paths), and `viv doctor` (host health check). `show` is a vague top-level name, `config` and `show` share a domain, and `doctor` also inspects config — so "inspect the setup and report" was smeared across three verbs with no clear boundary.

## Considered Options

- Keep all three verbs as-is (`show --resolved`, `config`, `doctor`)
- Fold inspection under `doctor` via flags; `config` becomes a thin `doctor` wrapper
- Retire `show`; make `config` the inspection namespace; keep `doctor` a pure checker

## Decision Outcome

Chosen option: **retire `show`; `config` owns config inspection; `doctor` stays a pure checker** — it splits the overlap into two honest domains: _inspect config_ vs _diagnose health_.

- `viv config` (no subcommand) — the binding: bound manifest + effective config/state/data/cache paths.
- `viv config sources [--json]` — provenance: declaring manifest and ordered pieces in merge order; the home for how merge-priority conflicts and ties render.
- `viv config eval [--json]` — the fully merged, **evaluated** config. Replaces `show --resolved`; `eval` names the semantic Nix step (echoes `nix eval`) instead of the meaningless `show`. Guards on the hard preflight subset (Nix present) since it evaluates; `config`/`config sources` are pure metadata reads.
- `viv doctor` is unchanged: a checker with pass/warn/fail and sysexit codes, never a config renderer.

Grounded in dominant CLI precedent: `nix config show`, `npm config list`, `terraform show`, `kubectl config view` render config, while `nix doctor`, `npm doctor`, `brew doctor`, `flutter doctor` only diagnose — the two are never one verb. All three `config*` paths are read-only (N13); exit codes follow ADR-0015.

## Consequences

- Good: two clear domains; `eval` names the real operation; `config sources` gives attribution a real home.
- Good: design-stage, so `show` is removed cleanly — no deprecation alias to carry.
- Bad: `config` becomes a command group with a default action, slightly more surface.

## Status

Accepted

Amends [`ADR-0015-cli-output-and-failure-contract.md`](./ADR-0015-cli-output-and-failure-contract.md) — retires `show`, adds the `config` inspection namespace. Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) and [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md).
