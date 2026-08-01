# ADR-0038: Guest store sharing — host store, read-only

## Context and Problem Statement

`spec/06` said the guest's Nix store "may either be independent or share the host's store read-only" and called it an implementation trade-off "made below the level of this contract." It is not: with several VMs running, that choice sets how much disk, page cache, and build time each additional VM costs — a user-visible scaling property, so it must be decided.

## Considered Options

- **Independent guest store** — a per-VM compressed store image built from the VM's closure.
- **Host store shared read-only** into the guest over the filesystem-sharing transport.
- Independent by default, with sharing as an opt-in.

## Decision Outcome

Chosen option: **the host store, shared read-only**, is the default.

- Each additional VM then costs **no duplicated store bytes and no store image to build**. With an independent store, N VMs hold N near-identical copies of the same closure, rebuilt whenever any layer changes — precisely the cost that makes running five projects expensive.
- The share is read-only on both sides and served by its own confined daemon, like every other share ([`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)).
- A **writable overlay** above it keeps in-guest builds working, so the two-layer design (ADR-0008) is unaffected.
- **The trade is stated, not hidden**: the guest can read every path in the host store. The store is world-readable by construction and never holds secrets (N10, ADR-0010), and the threat model is escape, not enumeration — so this is an information exposure about which packages the host has, not a weakening of the boundary (N1).
- An independent store remains admissible as a future hardened profile for genuinely untrusted work; it is not the default.

## Consequences

- Good: marginal disk per VM approaches zero and boot skips a store-image build.
- Good: one host page cache serves every guest's store reads.
- Bad: a guest can enumerate the host's installed closure.
- Bad: adds a second high-traffic share to keep confined and correct.

## Status

Accepted

The page-cache saving is host-side only, and the original wording overstated it. There is no cross-VM deduplication of guest memory — a structural consequence of sharing any host directory into a guest, not of this decision — so each guest still caches what it reads. `spec/17` states that constraint; the clause claiming N-times guest-memory caching as a cost of the rejected option has been struck, because that cost is unavoidable either way.

Amended by **ADR-0048** — the store share's guest mount and the daemon that serves it are vivarium's to configure, which is what lets its shutdown ordering be stated as a contract in spec/06.

**The saving is proven for the boot closure and does not extend to the inner layer.** Measured on a real host: only the guest system's own closure is registered in the guest's Nix database, so every other host store path is physically present in the share and formally unknown to the guest. The project's own inner `nix develop` therefore fails on a path it can see. This narrows the decision's reach rather than reversing it — the marginal-disk and page-cache savings above are unaffected, and both rest on the boot closure. Making the inner layer benefit too is open work, and the choice it turns on (whether registration happens on the build channel or the launch channel) is a real fork, not a detail. Register entry in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

Discharges the deferral in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), which owns the resulting contract. The cache policy for this share is fixed by [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md).
