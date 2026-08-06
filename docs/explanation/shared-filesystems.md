# Shared filesystems

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

vivarium shares three classes of host content into a guest: the project workspace, explicitly declared configuration or extra mounts, and the immutable host Nix store. Host source paths are supplied only at launch. Guest targets are stable and project-scoped, and sources representing host temporary or session directories are refused.

Each share is served by its own confined daemon. The daemon translates a fixed guest identity to the invoking host user so build outputs remain host-independent. Cache mode expresses coherency, not confinement, and is selected per share by the mechanism it enables. Worker-pool and descriptor settings form one capacity budget: pool size affects concurrent service, while explicit file-descriptor limits make the maximum safe load inspectable.

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
- Descriptor enactment belongs to [slice 002](../plan/slices/002-secure-launch-and-supervision/README.md).
