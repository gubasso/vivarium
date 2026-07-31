# ADR-0079: The security role is held solo, and the response windows are targets

## Context and Problem Statement

[`ADR-0078`](./ADR-0078-backend-advisory-response-is-a-released-pin-move.md) gave the backend security owner "a named backup" and stated response windows in the present tense. vivarium has one maintainer, so the backup slot cannot be filled — and the release gate that waited for it was a gate nobody could ever pass. A commitment with nobody behind it is worse than a weaker one that is kept.

## Considered Options

- Leave the backup unassigned and gate the first release on filling it
- Hold the role solo, disclose that there is no backup, and call the windows targets
- Drop the response windows entirely

## Decision Outcome

Chosen option: **hold it solo and disclose it** — the honest form of a promise one person keeps.

The role stays; the backup does not. `SECURITY.md` says in its own words that vivarium is maintained by one person, that the role has no backup, and that an advisory which misses a window says so. The response table's column becomes a **target**, not a guarantee. Nothing else in ADR-0078 moves: the two clocks, the boundary-over-severity trigger, and what an advisory must name are unchanged, because none of them depends on headcount.

The **acknowledgement** window is untouched. Acknowledging a report is a minutes-long act one person can keep; publishing a release is not, and conflating the two is what made the old wording overreach.

The release gate narrows to the owner cell alone. `none` in the backup cell is a disclosed fact, not an unfilled blank. A second maintainer edits the release-process table, never a decision.

Dropping the windows was rejected: a boundary-triggered clock is the substance of ADR-0078, and a policy that promises no timing gives a reporter nothing to plan against.

## Consequences

- Good: the policy states what one person can actually deliver, so a missed window is a disclosed exception rather than a broken promise.
- Bad: a reader gets a weaker assurance, and a maintainer's unavailability has no cover.

## Status

Accepted

Amends [`ADR-0078`](./ADR-0078-backend-advisory-response-is-a-released-pin-move.md). Specified in [`../../SECURITY.md`](../../SECURITY.md); the holder is recorded in [`../PUBLISHING.md`](../PUBLISHING.md).
