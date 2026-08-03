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

Bounds [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md), and is safe only because of [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md) — on a plain overlay this policy would whiteout host paths on a schedule the user never asked for. Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), with the thresholds in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).

**Unimplemented, and until it lands the guest never collects.** That is the honest interim posture, stated rather than left to read as though ADR-0088 had covered it: ADR-0088 makes a collection safe, which is not the same as arranging for one.

**The public prior art diverges here and is not followed.** [`shazow/agentspace`](https://github.com/shazow/agentspace) — the same topology ADR-0088 takes its configuration shape from — ships 8 GiB for this volume and no automatic collection at all. Its users can resize a host filesystem; a vivarium user is inside a fixed ceiling, so the same posture would turn a full volume into a stuck project.

**Split from [`ADR-0090`](./ADR-0090-the-guest-store-volume-is-listed.md) deliberately.** Both answer the same discovery — a volume that grows unbounded and invisibly — but bounding the growth and reporting the size are reversible independently: dropping the thresholds would not un-print the row, and hiding the row would not stop a collection. They were drafted as one record and split on review.
