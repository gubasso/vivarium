# ADR-0091: The store volume is provisioned for inodes, not only bytes

## Context and Problem Statement

[`ADR-0089`](./ADR-0089-the-guest-store-is-collected-on-space-pressure.md) bounds the store volume by free **space**: Nix's auto-collector polls `statvfs` for free blocks. But a Nix store is millions of small files, and `mke2fs.conf`'s default provisions one inode per 16 KiB. A denser store exhausts inodes while the volume still reports free space, and the trigger has no inode dimension with which to see it.

## Considered Options

- **Keep `mkfs` defaults** and let inode exhaustion surface as an ordinary `ENOSPC`.
- **Teach the collection trigger to watch inodes** as well as blocks.
- **Provision a denser inode ratio when the volume is created.**

## Decision Outcome

Chosen option: **provision at creation** — the store volume is made with one inode per 8 KiB; every other volume keeps the default.

- **The failure it prevents is uniquely confusing**: `ENOSPC` on a volume showing gibibytes free, from a tool the user did not invoke.
- **ADR-0089's trigger structurally cannot catch it.** Free blocks are what `min-free`/`max-free` reads; adding an inode dimension means patching upstream Nix, not configuring it.
- **The density is measured, not conventional.** A real host store holds 2,626,920 inodes across 28 GiB — one per 11 KiB, denser than the 16 KiB default. At that default a 32 GiB volume runs out of inodes near 22 GiB, below the ceiling [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md) sets.
- **Per volume, not global.** The home volume holds ordinary files and wants the default. This is the first property that differs between volumes, and so the first argument for provisioning each one separately.

## Consequences

- Good: the byte ceiling becomes the binding limit, which is the one the user was told about.
- Bad: the inode table costs about 3% of the volume, charged whether used or not.
- Bad: the number is argued from one host's store and stays provisional until the spike measures guest growth.
- Bad: ext4 cannot add inodes in place, so changing it later is a re-create — acceptable only because this volume is regenerable.

## Status

Accepted

Refines [`ADR-0089`](./ADR-0089-the-guest-store-is-collected-on-space-pressure.md) rather than reversing it: the collection policy is unchanged, and this closes a gap that policy cannot reach. Sits under [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md), which creates the volume, and inherits [`ADR-0037`](./ADR-0037-volume-disk-format-and-reclamation.md)'s sparse raw image. Specified in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).

**Unimplemented, and the number is a starting point rather than a result.** The design checklist carries `df -i` alongside `df -h` as a recorded measurement on both boots of the persistence spike, which is what turns one inode per 8 KiB from an argued figure into a measured one — the same treatment [`ADR-0089`](./ADR-0089-the-guest-store-is-collected-on-space-pressure.md) gives its own thresholds.

**Why the filesystem itself was not reconsidered.** ext4 stays. The kernel requires an overlay upper layer to support `trusted.*`/`user.*` extended attributes and to return a valid `d_type` from `readdir`; ext4's defaults give both, with no mount option to add. Whiteouts are character devices, so the upper layer must also permit `mknod` — which is the formal reason this layer must be a block-backed volume and cannot be a share, and is the same failure upstream microvm.nix records as `overlayfs: upper fs missing required features`. XFS would work but needs `-n ftype=1`, a default that has changed across versions and would be a silent, host-dependent trap in a filesystem this project's launcher creates on an arbitrary machine. The journal is kept: this volume carries the store **database**, and a half-applied metadata change after an unclean shutdown corrupts a merged view rather than costing a cache entry. "Regenerable" means losing the volume is cheap, not that a silently inconsistent one is.

**One hazard left open.** `mkfs.ext4` may leave the inode table lazily initialized, in which case the kernel zeroes it in the background after first mount — allocating the whole table in the host's sparse image without the guest writing anything. At this density that is around a gibibyte appearing unprompted, which interacts with the sparse-image promise [`ADR-0037`](./ADR-0037-volume-disk-format-and-reclamation.md) makes. The spike measures the image's allocated blocks before and after first boot; if it inflates, the table is initialized eagerly at creation instead.
