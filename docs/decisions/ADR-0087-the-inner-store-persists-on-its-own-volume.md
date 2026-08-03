# ADR-0087: The inner store persists on its own volume

## Context and Problem Statement

[`ADR-0084`](./ADR-0084-the-inner-layer-provisions-its-own-store.md) settled that the project's inner environment provisions its own store, and left open whether what it provisions survives a restart. Guest-side Nix writes land in the store overlay's upper layer, which is the ephemeral runtime layer, so a profile under the persistent home points at paths that are gone. [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) called it "not currently expressible".

## Considered Options

- **Keep the cost** — document the dangling profile as expected.
- **An inner store under `$HOME`**, carried by the default volume.
- **Persist the overlay's upper layer**, on a volume mounted before the store.

## Decision Outcome

Chosen option: **persist the upper layer, together with the store database that describes it.**

- **Guest changes surviving a restart is product intent.** ADR-0084 removed the host as a source for the inner environment, which leaves the guest's own store as the only place that behaviour can come from.
- **[`ADR-0080`](./ADR-0080-the-sandbox-is-disposable.md) already permits it and needs no amendment.** It names caches as continuity and says volumes persist "so a warm restart is cheap". The store is the largest cache a guest has and every path in it is regenerable from a substituter, so nothing here becomes a system of record.
- **The database persists with the bytes.** Validity is a database query, so persisting paths alone reproduces ADR-0084's own condition — physically present, formally unknown — this time self-inflicted.
- **A store under `$HOME` is refused.** It needs an explicit `--store`, breaking the two-layer rule that the inner environment works identically inside or outside a sandbox.
- **What is shared stays shared.** The host store remains the overlay's lower layer, and a path in the guest's registered system closure is still used in place.

## Consequences

- Good: profiles stop dangling, and an inner `nix develop` stops re-fetching every boot.
- Bad: the volume must mount in initrd, so it is a new kind, outside the first-boot machinery of [`ADR-0067`](./ADR-0067-volume-prune-and-first-boot-home.md).
- Bad: it grows, so the guest needs a working collector — the subject of [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md).

## Status

Implemented

**Measured on a real host, over two boots against one store volume.** On the warm boot a path the guest added on the cold boot was valid _before anything was written_, with its bytes still in the writable layer — and validity is a database query, so this decision's central claim is now a measurement rather than an argument. Both premises the record below calls unmeasured are closed with it: the volume mounts from the initrd ahead of the merged store, and the persisted database composes with the boot-time registration in the order the design requires. The sharing claim is closed as a number: **0 of 520** requisites of the running system were copied into the writable layer, on both boots. Recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

Closes the question [`ADR-0084`](./ADR-0084-the-inner-layer-provisions-its-own-store.md)'s `## Status` left open, and supersedes nothing: ADR-0084's refusal to bridge host store bytes into the inner layer stands unchanged, and this decision does not reach it. What persists is what the guest fetched or built for itself.

Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md). The mechanism is [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md); the two are separate because the store's persistence and the store type that makes persistence safe are separately reversible.

**Two premises were unmeasured when this was written; both are now measured, per the paragraph above.** They were: that a volume can be attached and mounted early enough to back the writable layer under the guest's initrd, and that the database and the boot-time closure registration compose in either order. The second turned out to be structural rather than arranged: the boot-time registration runs in stage 2, before systemd, so it always precedes the daemon opening the store.

The premise this decision rested on — that an inner build lands its closure in the upper layer rather than reusing lower bytes — **holds, and by a stronger mechanism than assumed.** Nix does not write into a store path; it deletes the destination first and then restores or renames into it, on both the substitution path (`LocalStore::addToStore`) and the build path (`deletePath` then `movePath`). So a path that is physically present below but invalid in the guest's database is not reused: it is unlinked through the merged view, which writes the very whiteout [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md) exists to prevent, and then copied in full. That hazard is therefore reachable from the ordinary write path and not only from a collection, which neither ADR said. Recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md); a host run now confirms a known mechanism rather than deciding an open question.

**What the sharing turns on, precisely.** Under `local-overlay` the check that spares a path is its validity in the **lower store's database**, not its physical presence. That database describes the boot closure and nothing else, so every other host path in the share is still copied up — correct behaviour, and the honest per-project cost of [`ADR-0084`](./ADR-0084-the-inner-layer-provisions-its-own-store.md).
