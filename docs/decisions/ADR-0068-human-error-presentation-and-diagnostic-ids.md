# ADR-0068: Human error presentation and diagnostic ids

## Context and Problem Statement

ADR-0015 requires every error to render **what / where / why / hint**, and ADR-0028 fixed the exit codes as a permanent API where "the code carries the category; stderr carries the instance". Neither says what that stderr text looks like, and the instance layer has no machine-matchable handle at all. This became load-bearing rather than cosmetic when ADR-0047 and ADR-0052 removed the schema version from the two files a user writes: an unknown-key **message** is now the entire compatibility surface, in both places, and both ADRs name that message as an open obligation.

## Considered Options

- Leave rendering to the implementation; keep only the four-part rule.
- Fix a rendering skeleton, and give each error an id with exit-code-grade stability.
- Fix a rendering skeleton, and give each error a **stable but non-branchable** diagnostic id.

## Decision Outcome

Chosen option: **a fixed skeleton plus a stable, non-branchable diagnostic id.**

- **Skeleton.** Four mandatory slots — `error[<id>]: <what>`, `--> <where>`, `why:`, `hint:` — plus one conditional `accepted here:` slot carried only by unknown-key and unknown-value failures. `where` degrades to a named locus when no file is involved.
- **Ids reuse the existing promise.** `spec/01` already guarantees "a stable check id" on preflight failure; rather than mint a third id space, one `<namespace>.<condition>` grammar covers every vivarium-origin failure and preflight ids carry over verbatim.
- **The code stays the branch surface.** The id is stable and greppable; consumers branch on the exit code. No `viv explain`, no `url:` slot, and no central index — each id is documented where its condition lives.
- **`--json` failures emit one semantic object on stderr**, never a captured human rendering.
- **The two compat messages are normative text**, and must name the file, the failing line, the unknown key, the accepted key set, and the CLI version.

## Consequences

- Good: discharges the obligation ADR-0047 and ADR-0052 both left open.
- Good: one id space serves `doctor`, preflight, and every other failure.
- Bad: a rendering skeleton is a compatibility surface of its own, however informally.

## Status

Accepted

Amends [`ADR-0015-cli-output-and-failure-contract.md`](./ADR-0015-cli-output-and-failure-contract.md) and [`ADR-0028-exit-code-taxonomy-and-stability.md`](./ADR-0028-exit-code-taxonomy-and-stability.md). Specified in [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md) and [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md).
