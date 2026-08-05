# ADR-0038: Guest store sharing — host store, read-only

## Context and Problem Statement

`spec/06` said the guest's Nix store "may either be independent or share the host's store read-only" and called it an implementation trade-off "made below the level of this contract." It is not: with several VMs running, that choice sets how much disk, page cache, and build time each additional VM costs — a user-visible scaling property, so it must be decided.

## Considered Options

- Independent guest store — a per-VM compressed store image built from the VM's closure.
- Host store shared read-only into the guest over the filesystem-sharing transport.
- Independent by default, with sharing as an opt-in.

## Decision Outcome

Chosen option: the host store, shared read-only, is the default.

- Each additional VM then costs no duplicated store bytes and no store image to build. With an independent store, N VMs hold N near-identical copies of the same closure, rebuilt whenever any layer changes — precisely the cost that makes running five projects expensive.
- The share is read-only on both sides and served by its own confined daemon, like every other share ([`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)).
- A writable overlay above it keeps in-guest builds working, so the two-layer design (ADR-0008) is unaffected.
- The trade is stated, not hidden: the guest can read every path in the host store. The store is world-readable by construction and never holds secrets (N10, ADR-0010), and the threat model is escape, not enumeration — so this is an information exposure about which packages the host has, not a weakening of the boundary (N1).
- An independent store remains admissible as a future hardened profile for genuinely untrusted work; it is not the default. Struck by ADR-0086 — see `## Status`.

## Consequences

- Good: marginal disk per VM approaches zero and boot skips a store-image build.
- Good: one host page cache serves every guest's store reads.
- Bad: a guest can enumerate the host's installed closure.
- Bad: adds a second high-traffic share to keep confined and correct.

## Status

Accepted

The page-cache saving is host-side only, and the original wording overstated it. There is no cross-VM deduplication of guest memory — a structural consequence of sharing any host directory into a guest, not of this decision — so each guest still caches what it reads. `spec/17` states that constraint; the clause claiming N-times guest-memory caching as a cost of the rejected option has been struck, because that cost is unavoidable either way.

Amended by ADR-0048 — the store share's guest mount and the daemon that serves it are vivarium's to configure, which is what lets its shutdown ordering be stated as a contract in spec/06.

The saving is proven for the boot closure and does not extend to the inner layer. Measured on a real host: only the guest system's own closure is registered in the guest's Nix database, so every other host store path is physically present in the share and formally unknown to the guest. The project's own inner `nix develop` therefore fails on a path it can see. This narrows the decision's reach rather than reversing it — the marginal-disk and page-cache savings above are unaffected, and both rest on the boot closure. The clause that called making the inner layer benefit too "open work" is struck: it is settled work, settled by refusal, in the amendment below. Register entry in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

The fork above is closed by refusing it, not by filling it in. [`ADR-0084-the-inner-layer-provisions-its-own-store.md`](./ADR-0084-the-inner-layer-provisions-its-own-store.md) rules that the inner layer provisions its own store and vivarium never bridges host bytes into it, superseding [`ADR-0083-inner-layer-store-registration-is-on-demand.md`](./ADR-0083-inner-layer-store-registration-is-on-demand.md), which had settled a policy for doing so. The decision here is unchanged and the measurement above still stands — but it is now a stated property of the design rather than open work: what this decision shares is the guest's own closure, and it was chosen over a per-VM store image on disk and build cost, never as a way to lend the host's packages to a project. Its wording above ("making the inner layer benefit too is open work") is superseded by that ruling. One correction to the measurement's framing: the inner `nix develop` failure it records is bounded by egress mode rather than universal, since egress is open by default ([`ADR-0007-default-open-egress.md`](./ADR-0007-default-open-egress.md)) and an inner environment then substitutes normally into the writable overlay.

The shared store must not change while a guest is running, and this decision never said so. Sharing the host store read-only makes it an overlayfs lowerdir inside the guest, and the kernel is explicit that changing an underlying filesystem while it is part of a mounted overlay leaves the overlay's behaviour undefined ([Linux overlayfs documentation](https://docs.kernel.org/filesystems/overlayfs.html)); Nix's own overlay store states the same requirement for its lower store. So a host `nix-store --gc` — or any store mutation — under a running guest is already outside the contract, independently of the inner-layer question. The obligation is a host-GC interlock for the lifetime of every running guest, not a per-closure root, and it is this decision's to own because it follows from sharing the live store at all. It therefore survives ADR-0084 untouched: it binds on a host whose user never invokes Nix, because what is exposed is the guest's own boot closure.

Amended by [`ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md`](./ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) — the interlock above now has a mechanism, so the obligation this decision states is discharged rather than merely recorded — and by a refusal rather than a registration: the generation a VM boots from is already a garbage-collection root (N14), so what was missing is that `viv generations prune` must not unlink it under a running VM, with `doctor`'s `store-roots-intact` reporting what a pin cannot prevent. The rule stated above is unchanged and remains the contract; what changes is that vivarium no longer breaks it on the user's behalf, which it did whenever a build's own space-triggered collection ran while another project's guest was up. Unimplemented; what the guest observes when the rule is broken is still unmeasured.

Amended by [`ADR-0086-per-vm-store-duplication-is-refused.md`](./ADR-0086-per-vm-store-duplication-is-refused.md) — the independent per-VM store this decision reserved is refused outright, in every profile. The reservation was recorded as a hardened-profile escape hatch and read for three years' worth of decisions as though the scaling property were a tunable. It is not. The rejected option above holds one copy of the base per sandbox where the chosen one holds a single copy in total, so adopting it would make vivarium cost more disk than the container model `explanation/disk-model-vs-containers.md` measures it against — inverting the property `spec/00` states as a goal rather than trading against it. The bullet reserving it is struck. Everything else here stands, including the enumeration exposure, which now has no remedy in any profile and remains the accepted trade this decision argues for.

A store shared writable between guests is refused, and the refusal is permanent. The obvious analogy — one Nix store volume, database included, mounted read-write into several sandboxes, the way container tooling does it — does not survive the boundary N1 mandates. SQLite requires that "all processes using a database must be on the same host computer", because its write-ahead log needs a small amount of shared memory that processes on separate machines cannot have ([SQLite WAL](https://www.sqlite.org/wal.html)); two guests are separate host computers by that definition, not by analogy. Nix's lock discipline degrades silently besides: virtiofsd defaults to `no_posix_lock` and `no_flock`, and upstream states plainly that "posix locks will work within applications in a guest but not across guests" ([virtiofsd(1)](https://manpages.ubuntu.com/manpages/jammy/man1/virtiofsd.1.html)). A shared block device is worse rather than better, because ext4 is not a cluster filesystem. The security half is decisive on its own: the read-only share above is an information exposure, but a writable one lets one guest write a store path that another guest then executes — a direct N1 breach, not a trade. Containers can share a store this way only because they share one kernel; that is the property the isolation boundary removes.

Discharges the deferral in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), which owns the resulting contract. The cache policy for this share is fixed by [`ADR-0039-share-cache-policy.md`](./ADR-0039-share-cache-policy.md).
