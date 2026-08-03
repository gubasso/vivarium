# ADR-0088: The guest store is a local-overlay store

## Context and Problem Statement

Nix's collector enumerates the store **directory** and deletes every entry not valid in its own database (`LocalStore::collectGarbage` — behaviour, not contract; the manual promises only reachability). Where that store is an overlay over the host's, deleting a lower-layer path writes a **whiteout**: the host is untouched, the guest blinds itself. Today those vanish at shutdown; [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md) makes them durable.

## Considered Options

- **A vivarium whiteout sweep** at boot.
- **Forbid guest-side collection.**
- **Nix's `local-overlay` store type** (experimental `local-overlay-store`).

## Decision Outcome

Chosen option: **configure the guest's store as a `local-overlay` store.**

- **Upstream owns this hazard by name.** `LocalOverlayStore::deleteStorePath` deletes a duplicated path through the upper layer "to avoid creating a whiteout", then remounts to clear the stale handles that causes.
- **A lower-only path is untouched**, because the guard is `pathExists(upperPath)` with no `else`. The catastrophic case — a collection whiteouting thousands of host paths the guest never registered — cannot occur.
- **The lower store needs a database, and it is not the host's.** The boot-time registration manifest ships with the image, so the guest builds its own view from its own closure. No host belief is imported, so ADR-0084 holds despite reading as a reversal.
- **"Experimental" is not the risk it reads as.** A dedicated functional-test suite runs in CI, and two years since release carry no corruption report. Pin the Nix package and assert it is CppNix — Lix does not implement it.
- **The sweep was a vivarium invention** for a problem upstream had solved, and left collection wrong between boots.

## Consequences

- Good: guest collection is correct by construction, and duplicates deduplicate against the lower layer.
- Bad: an experimental feature in the boot path, removable without notice.
- Bad: `check-mount` cannot pass — after `switch-root` the recorded `lowerdir` still names the initrd prefix — so a vivarium assertion replaces it.
- Bad: a remount hook is required, forcing the legacy mount API.
- Bad: a non-store-path name in the store root aborts a collection: it is parsed before the guard above is reached.

## Status

Accepted

Mechanism for [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md); separate from it because the store type is reversible without giving up persistence. Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md).

**Unimplemented.** The configuration shape is known from public prior art running this same topology — microVM guest, host store shared read-only as the overlay's lower layer, `local-overlay` above it ([`shazow/agentspace`](https://github.com/shazow/agentspace), which removed microvm.nix's plain writable-store overlay in favour of this). Carried in the design checklist with the four details that prior art records and a first attempt would otherwise pay for: `check-mount=false`, a second interposed overlay so the read-only lower store may create the `.links` directory `LocalStore` makes unconditionally, `LIBMOUNT_FORCE_MOUNT2=always` in the remount hook because the newer kernel mount API cannot remount overlayfs, and loading the database before the daemon opens the store read-only.

**Three claims in the body were wrong and are corrected in place.** The maturity bullet originally read "`nixpkgs` requires the flag for a rootless daemon, eleven tests run in CI, and three years carry no corruption report". The `nixpkgs` claim could not be found in any primary source and is dropped — the rootless-daemon precedent that does exist is the prior art's own check. The suite holds twelve outer scripts today, so it is described rather than counted. And the age was wrong: [PR #8397](https://github.com/NixOS/nix/pull/8397) merged 2024-04-08, and the implementation is absent at tags 2.18 through 2.22 and present at **2.23.0**, so the store type is about two years old. The two structural claims — the collector enumerating the directory, and the `pathExists(upperPath)` guard with no `else` — were re-verified against upstream and stand.

**A fifth prior-art detail, and one divergence from it.** The detail: `nix-daemon` is the only process that opens the overlay store, and clients reach it through the socket rather than opening it themselves. The divergence is the store share's cache policy — the prior art serves its lower layer with `auto` where vivarium uses `always` ([`ADR-0039`](./ADR-0039-share-cache-policy.md)). That combination is the one axis nobody has run, and the remount hook's sufficiency rests on it, because remounting the overlay is not obviously enough to invalidate the lower filesystem's own caches. If a guest spike shows `always` diverging, the share drops to `auto`: that costs ADR-0039's cache rationale, which was a performance argument, and does not reach this decision.

**This does not make a mutating host store safe.** The lower layer must still not change under a running guest ([`ADR-0038`](./ADR-0038-guest-store-sharing.md)), and persistence raises the cost of breaking that rule from one session to a stranded project — which strengthens the case [`ADR-0085`](./ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) leaves open for running `store-roots-intact` unprompted.

**A sixth flag, missing above: `read-only-local-store`.** The prior art enables `local-overlay-store` **and** `read-only-local-store` together, because the lower store is opened with `read-only=true` in its URI and that parameter is itself behind the second flag. This record named only the first. Both are required; enabling one alone fails at daemon start, not at evaluation.

**The prior art's module must be adapted, not vendored whole — and the line that must not be copied is named here so nobody re-derives the reason.** [`shazow/agentspace`](https://github.com/shazow/agentspace)'s `local-overlay-store.nix` sets `microvm.writableStoreOverlay = lib.mkForce null` and then hand-writes `fileSystems."/nix/store"` itself. That is integration glue for _its_ configuration, not one of the details above, and copying it into vivarium is a silent regression of [`ADR-0038`](./ADR-0038-guest-store-sharing.md)'s shutdown ordering. Upstream microvm.nix emits its own `nix-store.mount` drop-in with `What=store` under exactly three conditions — initrd systemd enabled, the store not on disk, and `writableStoreOverlay == null` — and vivarium satisfies the first two. Forcing the option null satisfies the third, and upstream's drop-in then wins over the `What=overlay` one that spec/06's shutdown-ordering contract requires. The prior art escapes this only because it never enables initrd systemd, so its first conjunct is false; it runs the same virtiofs-share topology otherwise. Keeping `writableStoreOverlay` non-null instead yields the identical overlay upstream already generates, marks the store volume `neededForBoot` without a second declaration, and leaves the correct drop-in uncontested — so the adapted module is _smaller_ than the vendored one, not a derivation from scratch. Measured by evaluating both shapes against vivarium's own guest; recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).
