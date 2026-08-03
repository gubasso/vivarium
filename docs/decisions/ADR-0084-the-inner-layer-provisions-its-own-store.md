# ADR-0084: The inner layer provisions its own store

## Context and Problem Statement

[`ADR-0038`](./ADR-0038-guest-store-sharing.md) shares the host store read-only, and a measurement found only the boot closure registered in the guest's database. [`ADR-0083`](./ADR-0083-inner-layer-store-registration-is-on-demand.md) read that as a defect to close. The premise was never argued: is reusing host bytes a goal at all?

## Considered Options

- **Bridge host validity into the guest** — ADR-0083's policy, pending a mechanism.
- **The inner layer provisions its own store** — the share delivers the boot closure and nothing more.

## Decision Outcome

Chosen option: **the inner layer provisions its own store.** vivarium never bridges host store bytes into it, and the ambition is abandoned rather than deferred.

- **Host-agnosticism outranks the saving.** Nix on the host is a build prerequisite and nothing more may be assumed: any operating system, a user who never invokes Nix, a store holding only vivarium's builds. A feature whose value scales with the host's installed packages pays only Nix-using hosts, so it cannot be contract.
- **The share is boot-closure delivery, not package sharing.** ADR-0038 chose it over a per-VM store image on disk and build cost; the guest's root filesystem _is_ host store paths, and package sharing was never what it bought.
- **The measured failure is smaller than recorded.** Egress is open by default ([`ADR-0007`](./ADR-0007-default-open-egress.md)), so an inner `nix develop` substitutes into the writable overlay; under `allowlist` the user allowlists the cache or declares the tool in the image.
- **What the image declares, the image guarantees.** The rest is the inner layer's and dies with the sandbox ([`ADR-0080`](./ADR-0080-the-sandbox-is-disposable.md)) — [`ADR-0008`](./ADR-0008-two-layer-separation.md)'s line, held without vivarium reading the project.

## Consequences

- Good: no wire protocol, no host Nix under guest control, no retention protocol, no experimental store type.
- Good: behaviour is identical on a host with twenty thousand store paths and on one with two hundred.
- Bad: an inner Nix environment fetches what the image does not declare. **Amended by ADR-0087 — see `## Status`.**
- Bad: under `allowlist` egress a Nix-shaped inner layer needs a cache allowlist entry.

## Status

Accepted

Amended by **ADR-0087 — the writable layer the inner store fetches into now persists**, so the `Bad:` consequence about re-fetching is a one-time cost rather than a per-boot one. It originally read "re-fetches each boot; the overlay it writes to is ephemeral and no volume shape carries it". The decision itself is untouched: what ADR-0087 persists is what the guest fetched for itself, never host bytes.

Supersedes [`ADR-0083`](./ADR-0083-inner-layer-store-registration-is-on-demand.md), whose policy, three candidate mechanisms, and host-spike obligation all close unbuilt.

**Closed by [`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md) — the inner store persists on a volume attached early enough to back the overlay's writable layer.** This paragraph originally left that open, and named the cost: guest-side Nix writes landed in the ephemeral runtime layer, so a profile installed there left symlinks in the persistent home pointing at paths that no longer existed. The alternative shape it also named — an inner store under the home the default volume already persists — was refused, because it needs an explicit `--store` and so would not work identically inside and outside a sandbox.

Nothing in the body below is weakened by that. The refusal is to bridging **host** store bytes into the inner layer, and what ADR-0087 persists is what the guest fetched or built for itself. The `Bad:` consequence about re-fetching each boot becomes a one-time cost rather than a per-boot one; the `allowlist` consequence stands unchanged.

**The host-GC interlock is untouched and still owed.** It follows from sharing the live store at all — the guest's own boot closure is read through an overlay lower layer — so it holds on a host with no inner-layer registration and no user Nix usage whatsoever. It remains [`ADR-0038`](./ADR-0038-guest-store-sharing.md)'s, stated in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and unimplemented.
