# Shared filesystems

The share launch profile and descriptor arithmetic are implemented in [`src/launch`](../../src/launch/mod.rs) and [`src/doctor/descriptors.rs`](../../src/doctor/descriptors.rs). Guest mounts remain Nix-owned, and public doctor rendering remains later CLI work; see [implementation status](../reference/implementation-status.md).

vivarium shares three classes of host content into a guest: the project workspace, explicitly declared configuration or extra mounts, and the immutable host Nix store. Host source paths are supplied only at launch. Guest targets are stable and project-scoped, and sources representing host temporary or session directories are refused.

Each share is served by its own confined daemon. The daemon translates a fixed guest identity to the invoking host user so build outputs remain host-independent. Cache mode expresses coherency, not confinement, and is selected per share by the mechanism it enables. Worker-pool and descriptor settings form one capacity budget: pool size affects concurrent service, while explicit file-descriptor limits make the maximum safe load inspectable.

The launch contract declares `VIRTIOFSD_RLIMIT_NOFILE = 524288`. The closed daemon constructor renders that value as `--rlimit-nofile=524288`, and the transient unit renders the same serialized value as `LimitNOFILE=524288`. Nothing inherits or probes the caller's shell limit as a replacement.

For pinned virtiofsd 1.14.0 the reserve is `609 + effective_worker_count`, where the daemon computes that second term as `max(thread_pool_size, 1)` because a worker opens descriptors before it is accounted for. ADR-0096 fixes the worker pool at `0`, which is still charged as one, so the current guest allowance is `524288 - (609 + 1) = 523678` descriptors. Re-verified on 2026-08-10 against v1.14.0 and the previously pinned v1.13.3, which agree; the earlier figure of `523679` read the declared pool as the effective one and overstated the allowance by one. The pure `host-fd-limit-sufficient` calculation returns the declaration, both reserve inputs, and the derived allowance; a limit at or below the reserve is invalid. Changing a test contract's one descriptor field changes the daemon argument, unit property, and doctor datum together.

The host store is exposed read-only. Guest writes never modify it and belong to the guest-local store overlay described in [guest store and volumes](./guest-store-and-volumes.md). Exact mount fields, source restrictions, identities, and cache values are owned by the [workspace specification](../reference/spec/06-workspace-and-project-environment.md) and [invariants](../reference/spec/08-invariants-and-guarantees.md).

## Governing decisions

- [ADR-0017](../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md) — fixes the stable workspace target and the extra-mount form.
- [ADR-0020](../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes the declarative schema those mounts are authored in.
- [ADR-0027](../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — fixes the confinement each sharing daemon runs under.
- [ADR-0038](../decisions/ADR-0038-guest-store-sharing.md) — exposes the host store read-only rather than copying it.
- [ADR-0039](../decisions/ADR-0039-share-cache-policy.md) — establishes that cache mode expresses coherency, not confinement.
- [ADR-0050](../decisions/ADR-0050-share-cache-policy-named-by-mechanism.md) — names each cache policy by the mechanism it enables.
- [ADR-0096](../decisions/ADR-0096-the-share-worker-pool-takes-the-daemon-default.md) — fixes the uniform worker pool at the daemon's default, superseding [ADR-0051](../decisions/ADR-0051-share-worker-pool-small-non-zero-uniform.md).
- [ADR-0066](../decisions/ADR-0066-share-uid-gid-translation.md) — translates a fixed guest identity to the invoking host user.
- [ADR-0092](../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md) — masks the host link farm inside the shared store view.
- [ADR-0093](../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md) — declares each daemon's descriptor limit so the safe load is inspectable.

## Unresolved

- Whether a pool ever wins when a request blocks on cold backing storage. The concurrent sweep that settled the constant ran with the host page cache warm throughout, so the head-of-line case is measured absent rather than measured harmless — the boundary is stated with the numbers in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).
