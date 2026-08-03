# ADR-0088: The guest store is a local-overlay store

## Context and Problem Statement

Nix's collector enumerates the store **directory** and deletes every entry not valid in its own database (`src/libstore/gc.cc`, `LocalStore::collectGarbage`; the manual promises only reachability, so this is behaviour, not contract). Where that store is an overlay over the host's, deleting a lower-layer path writes a **whiteout**: the host is untouched, the guest blinds itself. Today those vanish at shutdown; [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md) makes them durable.

## Considered Options

- **A vivarium whiteout sweep** at boot.
- **Forbid guest-side collection.**
- **Nix's `local-overlay` store type** (experimental `local-overlay-store`).

## Decision Outcome

Chosen option: **configure the guest's store as a `local-overlay` store.**

- **Upstream owns this hazard by name.** `LocalOverlayStore::deleteStorePath` deletes a duplicated path through the upper layer "to avoid creating a whiteout", then remounts to clear the stale handles that causes.
- **A lower-only path is untouched**, because the guard is `pathExists(upperPath)` with no `else`. The catastrophic case — a collection whiteouting thousands of host paths the guest never registered — cannot occur.
- **The lower store needs a database, and it is not the host's.** The boot-time registration manifest ships with the image, so the guest builds its own view from its own closure. No host belief is imported and ADR-0084 holds; say so, because it reads as a reversal.
- **"Experimental" is not the risk it reads as.** `nixpkgs` requires the flag for a rootless daemon, eleven tests run in CI, and three years carry no corruption report. Pin the Nix package and assert it is CppNix — Lix does not implement it.
- **The sweep was a vivarium invention** for a problem upstream had solved, and left collection wrong between boots.

## Consequences

- Good: guest collection is correct by construction, and duplicates deduplicate against the lower layer.
- Bad: an experimental feature in the boot path, removable without notice.
- Bad: `check-mount` cannot pass — after `switch-root` the recorded `lowerdir` still names the initrd prefix — so a vivarium assertion replaces it.
- Bad: a remount hook is required, forcing the legacy mount API.

## Status

Accepted

Mechanism for [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md); separate from it because the store type is reversible without giving up persistence. Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md).

**Unimplemented.** The configuration shape is known from public prior art running this same topology — microVM guest, host store shared read-only as the overlay's lower layer, `local-overlay` above it ([`shazow/agentspace`](https://github.com/shazow/agentspace), which removed microvm.nix's plain writable-store overlay in favour of this). Carried in the design checklist with the four details that prior art records and a first attempt would otherwise pay for: `check-mount=false`, a second interposed overlay so the read-only lower store may create the `.links` directory `LocalStore` makes unconditionally, `LIBMOUNT_FORCE_MOUNT2=always` in the remount hook because the newer kernel mount API cannot remount overlayfs, and loading the database before the daemon opens the store read-only.

**This does not make a mutating host store safe.** The lower layer must still not change under a running guest ([`ADR-0038`](./ADR-0038-guest-store-sharing.md)), and persistence raises the cost of breaking that rule from one session to a stranded project — which strengthens the case [`ADR-0085`](./ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) leaves open for running `store-roots-intact` unprompted.
