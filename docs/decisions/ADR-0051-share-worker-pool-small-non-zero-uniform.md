# ADR-0051: The share worker pool is small, non-zero, and uniform

## Context and Problem Statement

`spec/06` pins each filesystem daemon's worker-pool size explicitly but commits to no number, because the defaults in play disagree: the daemon ships with its pool disabled, the guest-module ecosystem uses one worker per host core, and the backend's guidance is that a share on fast storage needs enough workers to approach native throughput. The interim guidance was to pin the disabled default as the conservative baseline. It is not conservative.

## Considered Options

- Pin the pool disabled — the daemon's own default.
- One worker per host core — the guest-module ecosystem's default.
- **A small fixed non-zero pool, uniform across shares.**
- Per-share pool size, as with cache policy.

## Decision Outcome

Chosen option: **small, fixed, non-zero, uniform**.

- **The daemon exposes exactly one request queue per share**, so pool size is the only concurrency lever a share has. With the pool disabled, every filesystem request for that share executes on a single thread, in order, with one slow request blocking everything behind it — the wrong posture for a workspace, whose traffic is many small concurrent metadata requests.
- **The evidence behind the disabled default does not reach this case.** It measured a sixty-four-thread pool against none, in an earlier implementation of the daemon, and its own author later held that a pool wins for concurrent work. Nothing establishes that none beats a *small* pool.
- **One worker per host core is rejected**: vivarium multiplies it by the shares in a sandbox and again by the sandboxes running at once ([`ADR-0036-host-resource-scoping-and-admission-control.md`](./ADR-0036-host-resource-scoping-and-admission-control.md)).
- **Uniform, not per-share.** Cache policy is per-share because it is a correctness property; pool size carries none, so a per-share knob would multiply threads for no invariant.
- The value lives in the launch wrapper; the spec states the property instead, so a later measurement moves the constant without changing a specified sentence.

## Consequences

- Good: a share's concurrency is bounded and stated, and no upstream default change can flip it.
- Bad: the constant rests on mechanism plus one at-scale precedent, not yet on vivarium's own measurement.

## Status

Accepted

Amends [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md), which owns the rule that these daemon settings are performance rather than confinement, and under which worker-pool sizing sits. Applied in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md); no part of the N20 profile in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
