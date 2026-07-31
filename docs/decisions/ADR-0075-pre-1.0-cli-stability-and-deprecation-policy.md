# ADR-0075: Pre-1.0 CLI stability and deprecation policy

## Context and Problem Statement

vivarium is at `0.1.0` and every command is designed rather than implemented. Nothing says what a `0.x` release may break, so the honest default reading — "anything may change" — contradicts three surfaces earlier decisions already made permanent. A reader deciding whether to script against `viv` has nothing to consult.

## Considered Options

- Say nothing until 1.0
- Stable within a minor series, with the permanent surfaces named
- A per-feature experimental gate
- A graduation ladder per command

## Decision Outcome

Chosen option: **stable within a minor series**.

The minor component is the breaking axis before 1.0. A patch release must not break a documented invocation, a `--json` field, an exit code's meaning, or accepted manifest and registry syntax. Breaking changes land only in a new minor, each named in the release notes with a migration example. Where practical a surface is deprecated for one minor first; an immediate break is reserved for security, correctness, and anything never marked Implemented.

Three surfaces are permanent **now**, pre-1.0, because earlier decisions already made them so: the exit-code taxonomy, append-only and never reassigned (ADR-0028); the two compatibility messages and their five required parts (ADR-0068); and the registry record's two keys (ADR-0052). Diagnostic ids are stable and never reassigned, and remain not a branch surface — both halves hold.

Outside it: human stderr prose beyond those messages and their slots, additive `--json` fields, and the generated flake's internal shape.

No experimental gate ships before 1.0. At `0.x` the whole surface is unstable, so a per-feature gate adds a second stability vocabulary for no signal; `implementation-status.md`'s three levels already stop a skipped trial from reading as a promise.

## Consequences

- Good: patch upgrades become safe to automate, while minor releases keep the freedom a pre-1.0 design needs.
- Good: the permanent surfaces are stated in one place instead of inferred from four ADRs.
- Bad: every breaking change costs a minor bump and a migration note, including corrections nobody depended on.
- Bad: `implementation-status.md` becomes load-bearing, so it needs CI checks rather than goodwill.

## Status

Accepted

Specified in [`../reference/implementation-status.md`](../reference/implementation-status.md) and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
