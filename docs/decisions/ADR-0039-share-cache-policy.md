# ADR-0039: Share cache policy is coherency, not confinement

## Context and Problem Statement

ADR-0027 put a single filesystem-share cache mode inside the N20 hardening profile, beside the sandbox, seccomp, and per-share confinement that actually close the escape class behind CVE-2026-47243. Two things are wrong. The named mode is not a value the shipped daemon accepts — it belongs to an older generation of that daemon — so the profile as written would be rejected at launch. And filing a cache setting under hardening invites a future reader to defend it as a security control when it governs only coherency.

## Considered Options

- Keep one cache mode for every share, correcting only the value.
- Forbid caching everywhere, maximizing coherency.
- Set cache policy per share from that share's coherency requirement, and remove it from the hardening profile.

## Decision Outcome

Chosen option: per-share policy, outside the hardening profile.

- Cache mode is not a confinement mechanism. Confinement is the namespace sandbox, the seccomp filter, unprivileged execution, a read-only host source, and one daemon per share. Those stay exactly as ADR-0027 fixed them.
- The read-only store share caches aggressively. Its contents are content-addressed and immutable, so there is no stale view to guard against — this is a proof, not a trade-off, and it removes the whole toolchain from the round-trip path.
- Read-write shares use the daemon's default, bounded-timeout policy. Host-side edits become visible to the guest within that timeout, which is the behavior a user editing on the host and building in the guest expects.
- Caching is never disabled outright on the workspace. Directory walks and small-file metadata dominate this workload; forbidding client caching turns every path lookup into a round trip.

## Consequences

- Good: the launch profile becomes launchable, and the security argument stops carrying an unrelated passenger.
- Good: the metadata-heavy path gets the cache it needs.
- Bad: an edit made on the host is visible in the guest only after the timeout, not instantly.

## Status

Accepted

Amended by ADR-0049 — the daemon that accepts or rejects a cache policy is a member of the built runner's closure, so a policy it does not accept is an evaluation-time assertion, not the `doctor` probe this decision originally named.

Amended by ADR-0050 — the read-write policy is named explicitly rather than inherited as "the daemon's default", and is justified by disqualifying the other three policies on coherency mechanics rather than by a pending benchmark.

Amended by ADR-0051 — worker-pool sizing, which sits under this decision's performance-not-confinement rule, is pinned uniform across shares, because the daemon exposes a single request queue per share. Amended by ADR-0096 — the value that sizing takes is the daemon's own default, measured.

The aggressive policy is corroborated by the daemon's own stated precondition, and it is orthogonal to the overlay hazard [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md) flagged against it. virtiofsd documents `always` as selectable "only when the file system has exclusive access to the directory", and implements it as an 86400-second entry/attribute timeout plus a keep-cache hint on open — where the bounded policy uses one second. Every one of those is a statement about host-side changes becoming visible in the guest, which is precisely the case [`ADR-0038`](./ADR-0038-guest-store-sharing.md) already forbids as an invariant rather than mitigates as a risk. So the precondition the daemon asks for is one vivarium holds by decision, and three independent primary sources state the same rule: this project's ADR-0038, Nix's own `local-overlay` manual ("deleting or modifying store objects is not allowed"), and the kernel's overlayfs documentation ("changes to the underlying filesystems while part of a mounted overlay filesystem are not allowed").

None of it reaches the overlay's stale-file-handle failure, which upstream Nix attributes to deleting through the upper layer rather than to the lower filesystem's caching, and which concerns an ext4 block device with no FUSE in the path.

The axis is now run, and the expected null held. Two host boots of [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md)'s design, with this share on `always`, exercised both branches of the overlay's delete path — including the one that remounts the store — and produced no stale handle, no divergence, and a merged view that resolved to the lower inode after the remount. The `auto` arm was gated on a negative and therefore never ran: spending a boot to confirm what three primary sources already argued would have bought nothing. `always` stays. Recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md); the trigger for dropping this share to the bounded policy is unchanged.

Amends [`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — removing cache mode from the N20 profile without touching any other element of it. The concrete per-share table lives in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md); the launch recipe is in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).

The share's cost against a local volume is now measured — 2026-08-05. Over a byte-identical 50,000-file tree, `git status` through virtiofs took 271–304 ms against 58–68 ms on the guest's own ext4 volume (4.5–5×), and `rg` took 693–1058 ms against 296–343 ms (2.3–3×). That is the first measured figure behind "keep regenerable caches off the share", which had been argued from negative-lookup semantics alone.

The cache-placement leg did not discriminate, and it was a proxy for a cache workload rather than a cache workload: 62–81 ms with the tool cache on the share and on the volume alike, across every pool size. So the guidance keeps its semantic argument and gains a ratio, and the claim that the layout choice is "the larger win" stays unmade. Registered in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).
