# ADR-0050: Share cache policy is named by mechanism, not inherited

## Context and Problem Statement

ADR-0039 made cache mode per-share but left the read-write policy inherited — "the daemon's default" — pending a benchmark meant to choose among the daemon's four policies. ADR-0049 then made closure-determined values evaluation-time assertions, which vivarium can assert only if it names them. And the benchmark could never have decided this: three of the four policies fail on correctness, not speed.

## Considered Options

- Keep deferring to the daemon's default until the benchmark runs.
- Cache nothing on host-writable shares, maximizing visibility of host edits.
- **Name the bounded-timeout policy explicitly**, disqualifying the other three on correctness.

## Decision Outcome

Chosen option: **name it explicitly**.

- **The long-timeout policies are disqualified for any host-writable share.** They hold names and attributes for a day, and the daemon can neither push an invalidation to the guest nor propagate a host-side change notification. Timeout expiry is the only invalidation there is, so a host-side edit is not late — it is unobservable until remount.
- **The cacheless policies are disqualified on the very metric that would recommend them.** Both force direct I/O on regular files, so the guest kernel refuses shared memory-mapping — breaking executable loading, embedded databases, and toolchains that map their working files. The daemon's opt-out is documented as safe only under exclusive access to the directory, which a host-edited tree denies. The fully cacheless one also disables the bulk directory read.
- **The read-write workspace and read-only mirrored config use the bounded-timeout, close-to-open policy**, named at launch.
- **The store share keeps aggressive caching, with a sharper proof than immutability alone**: content-addressed paths are never rewritten in place, and no failed lookup is ever cached, so a store path created on the host mid-session is still found on first reference.

## Consequences

- Good: the choice rests on stated mechanism, is assertable at evaluation, and frees the pending measurement to size cost rather than pick policy.
- Bad: ADR-0039's trade-off is unchanged — a host-side edit is visible only after the timeout, now with its alternatives ruled out rather than deferred.

## Status

Accepted

Amends [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md), whose per-share outcome stands: what changes is that the read-write policy is named and justified by the daemon's coherency mechanics instead of deferred to a default. The per-share table lives in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md); the exclusion from the N20 profile is unchanged in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
