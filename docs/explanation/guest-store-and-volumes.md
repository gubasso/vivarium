# Guest store and volumes

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

Declared volumes provide explicit guest durability. A project receives a persistent home volume and may declare named volumes; stop and rebuild preserve them, while removal requires an explicit destructive command. First boot establishes guest ownership once, and pruning targets orphaned volumes rather than active project state.

The guest Nix store combines the immutable host store as a lower layer with a guest-local persistent upper layer and database. The inner development environment provisions its own store rather than bridging arbitrary host bytes into it. A running guest pins paths it reads. The guest masks the host link farm, persists upper data with its database, provisions the store volume for both bytes and inode demand, and collects on free-space pressure.

Collection and reclamation are distinct. The collector decides which paths may be deleted; the filesystem and sparse backing file determine whether allocated host blocks return. Exact thresholds, output fields, volume shapes, and command semantics live in the [workspace](../reference/spec/06-workspace-and-project-environment.md), [lifecycle](../reference/spec/10-vm-lifecycle.md), and [resource](../reference/spec/17-resources-and-capacity.md) specifications. Host measurements remain in the [verification harness](../reference/microvm-verification-harness.md).

## Governing decisions

- [ADR-0019](../decisions/ADR-0019-volume-model.md) — fixes the default home volume plus named volumes.
- [ADR-0037](../decisions/ADR-0037-volume-disk-format-and-reclamation.md) — fixes the sparse disk format and how blocks are reclaimed.
- [ADR-0038](../decisions/ADR-0038-guest-store-sharing.md) — shares the host store read-only, which is the lower layer here.
- [ADR-0041](../decisions/ADR-0041-resource-and-volume-channel-classification.md) — puts volume declarations on the launch channel.
- [ADR-0067](../decisions/ADR-0067-volume-prune-and-first-boot-home.md) — limits pruning to orphans and settles first-boot home ownership.
- [ADR-0080](../decisions/ADR-0080-the-sandbox-is-disposable.md) — makes guest state disposable, which is why durability must be declared.
- [ADR-0082](../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) — establishes that a return is read on the backing object, the same rule reclamation follows here.
- [ADR-0084](../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md) — makes the inner layer provision its own store, superseding the on-demand registration of [ADR-0083](../decisions/ADR-0083-inner-layer-store-registration-is-on-demand.md).
- [ADR-0085](../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) — pins the paths a running guest reads so a collection cannot take them.
- [ADR-0086](../decisions/ADR-0086-per-vm-store-duplication-is-refused.md) — refuses a per-VM store copy, permanently rather than for now.
- [ADR-0087](../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md) — persists the upper layer and its database on a dedicated volume.
- [ADR-0088](../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md) — fixes the store type as a local-overlay store rather than a plain overlay.
- [ADR-0089](../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md) — triggers collection on free-space pressure.
- [ADR-0090](../decisions/ADR-0090-the-guest-store-volume-is-listed.md) — makes the store volume visible in volume output.
- [ADR-0091](../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md) — provisions that volume for inode demand, which the space trigger cannot see.
- [ADR-0092](../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md) — masks the host link farm so a collection stays inside the guest's own store.

## Unresolved

- Space-triggered collection does not bound the store volume while [KI-0001](../reference/known-issues/KI-0001/README.md) is open: the pinned collector ends every pass after one path, on an uninitialised byte count it reads for a lower-only path. The trigger and its arithmetic are measured exact; the pass that follows does nothing. Whether vivarium guards against this, and how, is undecided and needs its own record.
