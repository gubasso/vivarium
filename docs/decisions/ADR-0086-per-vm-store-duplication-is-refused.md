# ADR-0086: Per-VM store duplication is refused

## Context and Problem Statement

[`../reference/spec/00-goals-and-non-goals.md`](../reference/spec/00-goals-and-non-goals.md) makes "cheap to run several" a goal and calls it contract: no per-VM store image, no duplicated store bytes. Yet [`ADR-0038`](./ADR-0038-guest-store-sharing.md) reserved an independent per-VM store as an admissible hardened profile, and [`ADR-0085`](./ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) made it a fallback. A reservation of that size is not a profile.

## Considered Options

- **Keep the reservation** — an opt-in hardened profile, and a safety fallback.
- **Refuse per-VM store duplication permanently.**

## Decision Outcome

Chosen option: **refuse it permanently.** No profile, no flag, no fallback.

- **It would make vivarium worse than the model it is measured against.** An OCI image is stored once and every container shares every layer; a per-VM store image shares nothing, so five sandboxes hold five copies where five containers hold one. [`../explanation/disk-model-vs-containers.md`](../explanation/disk-model-vs-containers.md) claims vivarium _exceeds_ that model, sharing at store-path granularity and materializing no per-VM image. Duplication does not trade against that claim — it inverts it.
- **Sublinear disk in the number of sandboxes is the premise, not an optimization.** Nix was never only a determinism choice; sharing by construction is what makes the fifth project nearly free. Without it, what remains is a VM launcher.
- **A setting that multiplies disk by N is a different product**, not a profile.
- **The refusal has teeth.** If ADR-0085's interlock proves insufficient, the answer is a better interlock — never duplication. Should none be findable, that is grounds to reconsider the architecture, not to fall back.
- **Store enumeration keeps no remedy** and stays ADR-0038's argued trade: the store is world-readable by construction, never holds secrets (N10), and the boundary protects against escape, not against a guest learning what a host has.

## Consequences

- Good: the `spec/00` goal becomes enforceable rather than aspirational, and one admissible store shape replaces two.
- Good: no future design can quietly buy safety with duplicated bytes.
- Bad: a user wanting store enumeration closed is not served.
- Bad: ADR-0085 loses its fallback, so an unsolvable interlock escalates to an architecture question, not a configuration one.

## Status

Accepted — the project owner's ruling, recorded as a standing constraint rather than a one-time choice.

Amends [`ADR-0038`](./ADR-0038-guest-store-sharing.md): its clause reserving "an independent guest store … as a future hardened profile for genuinely untrusted work" is **struck**. Amends [`ADR-0085`](./ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md): its per-VM-store-image fallback is **withdrawn**. Both decisions otherwise stand unchanged; what is removed is an escape hatch neither of them needed.

This does not narrow what a guest store may _be_ beyond what was already decided. The shared read-only host store remains the one admissible shape, and a store shared **writable** between guests remains refused for the independent reasons ADR-0038 gives.
