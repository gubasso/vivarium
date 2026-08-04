# ADR-0089: The guest store is collected on space pressure

## Context and Problem Statement

[`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md) gives the guest store a volume that survives a restart, and [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md) makes collecting inside it correct. Nothing invokes it. So the volume grows to its ceiling, and it is the one volume a user never declared and cannot see the size of — the failure arrives as an inner build out of space, with no row to point at.

## Considered Options

- **A sweep at boot**, before the session starts.
- **A periodic timer** inside the guest.
- **Nix's own `min-free`/`max-free` collection**, triggered by free space.

## Decision Outcome

Chosen option: **collect on free-space pressure.**

- **Space is the pressure; wall-clock is uncorrelated with it.** `min-free`/`max-free` runs in the daemon's own build loop and polls `statvfs` on the store directory. Over an overlay that reports the **upper** filesystem, so it measures the store volume and nothing else, and it dispatches virtually — `local-overlay` inherits it unchanged.
- **A boot sweep is refused outright.** It deletes the warm cache ADR-0087 exists to create, and charges the cost while the user waits.
- **The numbers reuse what exists**: [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md)'s 32 GiB per-volume default rather than an exception, with `min-free = 4 GiB` and `max-free = 8 GiB` — one Rust toolchain of headroom (rustc and cargo measure ~1.6 GiB of closure each), ~4 GiB reclaimed per pass, no thrash. Provisional until the persistence spike measures real growth.
- **The trigger is all that is decided here.** Making the resulting size visible is a separate, separately reversible question, answered in [`ADR-0090`](./ADR-0090-the-guest-store-volume-is-listed.md).

## Consequences

- Good: growth is bounded by the guest itself rather than by a ceiling the user meets as a failure.
- Good: `viv volume rm` stays the hammer, and no new command is added.
- Bad: a pass can land mid-build, so a session occasionally pays a re-fetch it did not cause.
- Bad: the thresholds are argued, not measured; wrong ones thrash or never fire.

## Status

Accepted

Amended by [**ADR-0091**](./ADR-0091-the-store-volume-is-provisioned-for-inodes.md) — this trigger reads free **blocks**, so it cannot see inode exhaustion, and the store volume is therefore provisioned with a denser inode ratio at creation. The collection policy here is unchanged; what is added is a limit it was never able to reach.

Bounds [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md), and is safe only because of [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md) — on a plain overlay this policy would whiteout host paths on a schedule the user never asked for. Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), with the thresholds in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).

**Implemented 2026-08-04** as two `nix.settings` lines in the guest module, replacing the interim posture recorded here before — that the guest never collected at all, because ADR-0088 makes a collection _safe_, which is not the same as arranging for one.

**The thresholds themselves remain argued, not measured.** Landing them is not the same as testing them: reaching either one needs a workload rather than a boot, since a boot never approaches 4 GiB of pressure. Still open is whether the trigger fires where this record predicts and frees roughly what it predicts, and what bytes-per-inode the guest's own store actually reaches — the first run to answer that also doubles as this policy's implementation test.

**The public prior art diverges here and is not followed.** [`shazow/agentspace`](https://github.com/shazow/agentspace) — the same topology ADR-0088 takes its configuration shape from — ships 8 GiB for this volume and no automatic collection at all. Its users can resize a host filesystem; a vivarium user is inside a fixed ceiling, so the same posture would turn a full volume into a stuck project.

**Split from [`ADR-0090`](./ADR-0090-the-guest-store-volume-is-listed.md) deliberately.** Both answer the same discovery — a volume that grows unbounded and invisibly — but bounding the growth and reporting the size are reversible independently: dropping the thresholds would not un-print the row, and hiding the row would not stop a collection. They were drafted as one record and split on review.

**The trigger is measured, and its arithmetic is exact — 2026-08-04.** `scripts/store-pressure-check --arm c` drove the daemon's free-space reading down through upstream Nix's own `_NIX_TEST_FREE_SPACE_FILE` hook. The collector did not fire at 16, 8, 6, 5 or 4 GiB, fired at 3 GiB asking for 5,368,709,120 bytes, and fired at 2 GiB asking for 6,442,450,944. So it fires **strictly below** `min-free` — 4 GiB exactly is not a crossing — and the amount is exactly `max-free` minus available, which is this record's "collect until `max-free` is free again" confirmed on a running daemon. The premise underneath it is measured too: across every sample, `statvfs` on the merged `/nix/store` reported the same free blocks as the upper filesystem, so the trigger does read the store volume and nothing else. Registered in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

**What is still not measured, and why the obvious experiment does not reach it.** With free space faked, `availAfterGC` is faked too, so the above proves the trigger and the arithmetic and **not** real reclamation — whether a real pass on real ext4 frees what it asked for, and whether the `remountIfNecessary()` that follows disturbs an in-flight build. The attempt to get there by moving `min-free` close to a real volume's free space failed for a mechanical reason worth recording: `autoGC` reads `settings.minFree` **inside the daemon**, and neither a client-side `--option min-free` nor `NIX_USER_CONF_FILES` on the daemon unit reaches it. The only route proven to reach it is `nix.settings` at image-build time. Closing this needs a measurement image built with scaled thresholds; it is not reachable by a runtime override, and 3 GiB of real ballast on a 32 GiB volume never approaches the real 4 GiB threshold.

**The thresholds themselves are still argued.** The numbers above validate the _mechanism_ at the configured values, not the _choice_ of 4 GiB and 8 GiB, which remains the sizing argument this record made from one Rust toolchain of headroom.
