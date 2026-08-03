# ADR-0092: The guest masks the host's link farm

## Context and Problem Statement

`LocalStore` computes its optimisation directory as `realStoreDir/.links`, and for a [`local-overlay`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md) store `realStoreDir` is the **merged** `/nix/store`. Since [`ADR-0038`](./ADR-0038-guest-store-sharing.md) makes the host's own store the lower layer, that path resolves to the **host's** link farm. Every collection `lstat`s each of its entries and `unlink`s any whose link count is one. Measured on the first host boot: the guest exhausted virtiofsd's per-guest descriptor budget before one collection finished.

## Considered Options

- **Leave it.** Accept a collection whose cost is the host's link count.
- **An opaque `.links` directory** in the writable layer, set in the initrd.
- **Mask the path with an empty tmpfs.**

## Decision Outcome

Chosen option: **mount an empty tmpfs at `/nix/store/.links`.**

- **The walk becomes constant-time**, and the guest's link farm becomes its own rather than a view of the host's.
- **Nothing is lost.** `.links` serves store optimisation alone, which upstream microvm.nix already asserts is off whenever a writable overlay is configured, and hardlinking across overlay layers cannot work anyway.
- **The collector already skips the directory by name** when it enumerates the store, so masking it cannot disturb what a collection collects.
- **A mount beats an opaque xattr.** overlayfs reads `trusted.overlay.opaque` at lookup, so the xattr has to be set before the overlay is assembled and stays correct only if nothing looked first; a mount is unconditional and needs no `attr` in the initrd.
- **It also stops a write.** `unlink` of a lower-only link went through the merged view, so the host's farm was being whiteouted into the guest's writable layer.

## Consequences

- Good: a guest collection costs what the guest's own store costs.
- Bad: the mount point must exist below, which it does on any host whose Nix created it — an assumption, not a guarantee.
- Bad: one more mount in the boot path, ordered by `RequiresMountsFor` on the daemon.

## Status

Accepted

Forced by measurement rather than argued from source: the first host boot of [`ADR-0088`](./ADR-0088-the-guest-store-is-a-local-overlay-store.md)'s design failed here, and the cause was then confirmed in Nix's `LocalStore` constructor (`linksDir = realStoreDir / ".links"`) and in `gc.cc`'s `removeUnusedLinks`. Recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

**Implemented**, and measured on both boots of the persistence spike: a rooted collection completed in seconds, the lower layer's entry count was unchanged across it, and no descriptor exhaustion occurred.

**Why the public prior art does not carry this.** [`shazow/agentspace`](https://github.com/shazow/agentspace) runs the same store type over the same overlay, but its lower layer is microvm.nix's generated store _image_, which contains store paths and nothing else. vivarium shares the host's literal `/nix/store` ([`ADR-0038`](./ADR-0038-guest-store-sharing.md)), so `.links` is present in the lower layer here and absent there. The gap is a consequence of vivarium's sharing decision, not of the store type.
