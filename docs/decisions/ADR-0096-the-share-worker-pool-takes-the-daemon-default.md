# ADR-0096: The share worker pool takes the daemon's default

## Context and Problem Statement

[`ADR-0051`](./ADR-0051-share-worker-pool-small-non-zero-uniform.md) pinned each filesystem daemon's worker pool at a small non-zero number, on the mechanism that a disabled pool serialises a share's single request queue and blocks every request behind a slow one. It recorded its own weakness: the constant rested on that mechanism plus one at-scale precedent, and on no measurement of vivarium's own. The measurement now exists, and it contradicts the mechanism.

## Considered Options

- Keep a small fixed non-zero pool.
- Take the daemon's own default, which is the pool disabled.
- Pick a size per workload class.

## Decision Outcome

Chosen option: the daemon's default — no pool size above it won a single measured cell, and a default that upstream also ships is the one a later measurement can move without an argument about provenance.

- Measured across `{0, 1, 2, 4}` and `{1, 4, 16}` concurrent clients, two lane runs, eight boots. At one and four clients the ordering is monotone in the pool size and pool 0 is fastest; at sixteen the sizes do not separate. Recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).
- The serialisation argument does not survive the numbers: pool 0 went from 642 ms to 211 ms as clients went from one to four, so one serving thread pipelines a full queue rather than stalling behind it.
- Per-workload sizing is refused for the predecessor's own reason: pool size carries no correctness property, so a knob would multiply threads for no invariant. Everything else that record settled carries forward unchanged.

## Consequences

- Good: fewer threads per share, per sandbox, and a measured cost removed.
- Good: the value matches the daemon's default, so no upstream change can flip it silently.
- Bad: the case a pool exists for — a request that blocks long enough to hold the queue — is not present in this measurement. The host page cache was warm throughout, so this is measured on cache-hot metadata traffic and on one host.

## Status

Implemented

Supersedes [`ADR-0051-share-worker-pool-small-non-zero-uniform.md`](./ADR-0051-share-worker-pool-small-non-zero-uniform.md), which carries the same reasoning for the value it chose and stays as the record of it. Amends [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md) in the same way its predecessor did, which owns the rule that these daemon settings are performance rather than confinement.

Enacted 2026-08-05 by `virtiofsdThreadPoolSize` in [`nix/default.nix`](../../nix/default.nix), asserted by the build-to-launch contract in [`nix/contract.nix`](../../nix/contract.nix). Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), which states the property and not the number, so this change moves a constant and not a specified sentence.

Revisit on a measurement with cold backing storage, or on a host whose share traffic blocks. `scripts/share-benchmark-check` is the lane, and its concurrent sweep is the leg that would show it.
