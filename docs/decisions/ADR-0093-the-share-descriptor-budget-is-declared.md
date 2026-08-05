# ADR-0093: The share descriptor budget is declared, not inherited

## Context and Problem Statement

[`ADR-0027`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) has the launch wrapper raise each filesystem daemon's soft descriptor limit to the hard limit before `exec`. That makes the ceiling whatever the invoking session happens to have, so the same project gets a different budget on a different host — and `host-fd-limit-sufficient` was written to warn about a number nobody declared. Exhausting it is not hypothetical: one guest walk did, mid-session.

## Considered Options

- Keep inheriting the hard limit, and warn when it looks low.
- Measure the ceiling by walking a large tree, and calibrate the warning against what that pins.
- Declare the limit on the daemon's own command line and derive the warning from it.

## Decision Outcome

Chosen option: declare the limit.

- The wrapper passes the descriptor limit explicitly. The launch profile already names every value it depends on rather than inheriting it; this is one more.
- The budget is then arithmetic, not an observation. The daemon subtracts a fixed internal reserve from its limit and hands the remainder to the guest, so the guest allowance follows from the declared limit and the pinned worker-pool size, and the health check needs no measured walk.

## Consequences

- Good: the ceiling stops varying by host, and the health check has a number to check against.
- Good: an open measurement becomes an assertion.
- Bad: a declared limit can be declared too low, and the reserve is upstream's to change.

## Status

Accepted

Amends [`ADR-0027`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — that record's "raises the soft descriptor limit to the hard limit before `exec`" is replaced by an explicit limit on the daemon's own arguments. Everything else in that profile, including the forgoing of inode file handles that makes the budget matter, stands unchanged.

Specified in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md); the derivation and the measurement confirming it are registered in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

Not implemented. The launcher passes no explicit limit, so the budget is inherited today.
