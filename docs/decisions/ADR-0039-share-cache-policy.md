# ADR-0039: Share cache policy is coherency, not confinement

## Context and Problem Statement

ADR-0027 put a single filesystem-share cache mode inside the N20 hardening profile, beside the sandbox, seccomp, and per-share confinement that actually close the escape class behind CVE-2026-47243. Two things are wrong. The named mode is not a value the shipped daemon accepts — it belongs to an older generation of that daemon — so the profile as written would be rejected at launch. And filing a cache setting under hardening invites a future reader to defend it as a security control when it governs only coherency.

## Considered Options

- Keep one cache mode for every share, correcting only the value.
- Forbid caching everywhere, maximizing coherency.
- **Set cache policy per share from that share's coherency requirement**, and remove it from the hardening profile.

## Decision Outcome

Chosen option: **per-share policy, outside the hardening profile**.

- **Cache mode is not a confinement mechanism.** Confinement is the namespace sandbox, the seccomp filter, unprivileged execution, a read-only host source, and one daemon per share. Those stay exactly as ADR-0027 fixed them.
- **The read-only store share caches aggressively.** Its contents are content-addressed and immutable, so there is no stale view to guard against — this is a proof, not a trade-off, and it removes the whole toolchain from the round-trip path.
- **Read-write shares use the daemon's default, bounded-timeout policy.** Host-side edits become visible to the guest within that timeout, which is the behavior a user editing on the host and building in the guest expects.
- **Caching is never disabled outright on the workspace.** Directory walks and small-file metadata dominate this workload; forbidding client caching turns every path lookup into a round trip.

## Consequences

- Good: the launch profile becomes launchable, and the security argument stops carrying an unrelated passenger.
- Good: the metadata-heavy path gets the cache it needs.
- Bad: an edit made on the host is visible in the guest only after the timeout, not instantly.

## Status

Accepted

Amends [`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — removing cache mode from the N20 profile without touching any other element of it. The concrete per-share table lives in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md); the launch recipe and its probe are in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
