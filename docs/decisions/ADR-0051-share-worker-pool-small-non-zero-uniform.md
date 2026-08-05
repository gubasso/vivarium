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
- **The evidence behind the disabled default does not reach this case.** It measured a sixty-four-thread pool against none, in an earlier implementation of the daemon, and its own author later held that a pool wins for concurrent work. Nothing establishes that none beats a _small_ pool.
- **One worker per host core is rejected**: vivarium multiplies it by the shares in a sandbox and again by the sandboxes running at once ([`ADR-0036-host-resource-scoping-and-admission-control.md`](./ADR-0036-host-resource-scoping-and-admission-control.md)).
- **Uniform, not per-share.** Cache policy is per-share because it is a correctness property; pool size carries none, so a per-share knob would multiply threads for no invariant.
- The value lives in the launch wrapper; the spec states the property instead, so a later measurement moves the constant without changing a specified sentence.

## Consequences

- Good: a share's concurrency is bounded and stated, and no upstream default change can flip it.
- Bad: the constant rests on mechanism plus one at-scale precedent, not yet on vivarium's own measurement.

## Status

Accepted

Amends [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md), which owns the rule that these daemon settings are performance rather than confinement, and under which worker-pool sizing sits. Applied in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md); no part of the N20 profile in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).

**Benchmarked across {0, 1, 2, 4} on a real host, 2026-08-05 — and the constant does not move, because the workload cannot move it.** Four boots sharing one guest closure, a 50,000-file tree, the guest page cache dropped before every repetition. The metadata leg (`git status` over virtiofs) does not separate the pool sizes at all: the spread within one pool's repetitions is wider than the spread between pools. The content leg (`rg`) does separate them and runs _backwards_ — 693 ms at pool 0 against 1058 ms at pool 4, with tight repetitions.

**That is not evidence for disabling the pool.** A non-zero pool exists to serve **concurrent** requests, and both workloads are a single process, so neither exercises the head-of-line blocking that a disabled pool imposes. What the numbers show is per-request dispatch overhead with nothing to overlap — a real cost, and not the case this decision turns on. The prescription of `git status` + `rg` therefore cannot settle this constant; a concurrent workload is needed, and that is the open item. `4` stands, now with a measured cost attached rather than none. Registered in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

**Two documentation obligations, discharged here.** `--inode-file-handles=never` is justified as **determinism**, not descriptor economy: the daemon's own default is `prefer` and it downgrades to `never` at startup whenever a handle cannot be opened, so the two are behaviourally identical under this profile and pinning `never` is what stops that changing silently. And the argument that a pool of 4 costs three more reserved descriptors than 1 — out of roughly 523,000 — is noise and must not appear in any defence of this constant.
