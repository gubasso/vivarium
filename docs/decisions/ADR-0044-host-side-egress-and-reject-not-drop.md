# ADR-0044: Egress enforcement is host-side, and denials are rejected rather than dropped

## Context and Problem Statement

ADR-0007 chose default-open egress with an opt-in allowlist and said an **in-guest** firewall enforces it. [`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md) says the opposite — enforcement is host-side, because the guest is treated as adversarial and may hold guest-root, so an in-guest rule set could simply be torn down. No ADR ever recorded that reversal, so the only decision record still points an implementer at the wrong side of the boundary. Separately, spec/05 asked only that a denial be "legible" using **should**, which [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md) makes non-normative — so nothing was actually required of a blocked connection.

## Considered Options

- Keep enforcement in the guest and accept that guest-root can disable it.
- Enforce host-side but leave the denial surface entirely to implementation.
- Enforce host-side and require denials to fail fast rather than be discarded.

## Decision Outcome

Chosen option: **host-side enforcement with a required reject-not-drop denial surface.**

- The allowlist is enforced on the **host end** of the guest's network path. A rule set inside the guest is not a boundary against a guest that may be compromised; the wall must sit where the guest cannot reach it (N8's knob is unchanged — this is about where it is applied).
- A denied connection **must** fail the guest's `connect()` promptly with an error, and a denied name's DNS lookup **must** fail the same way. It must never be silently discarded. A dropped packet turns a policy decision into an unexplained hang, which reads as a broken sandbox rather than an enforced rule.
- The enforcement **mechanism** stays open — nftables, a tap/NAT firewall, or a user-mode filter all satisfy this, so the networking backend design remains free to choose.

## Consequences

- Good: an implementer reading the decision record now builds the enforceable design.
- Good: the denial surface is testable without pinning a mechanism — a denied fetch fails quickly instead of hanging.
- Bad: no vivarium exit code describes a denial; after guest-process start `exec` returns the guest program's own status verbatim, so the observable is the guest tool's error.
- Bad: reject-not-drop is marginally more informative to an adversary than a silent drop.

## Status

Accepted

Amends [`ADR-0007-default-open-egress.md`](./ADR-0007-default-open-egress.md). Specified in [`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md).

Extended by [`ADR-0064-egress-allowlist-enforcement-model.md`](./ADR-0064-egress-allowlist-enforcement-model.md), which supplies the mechanism this ADR deliberately left open and makes both of its rules concrete: "host-side" becomes a per-VM network namespace the guest has no handle on, and "reject, not drop" becomes DNS `REFUSED`, TCP reset, and ICMP administratively-prohibited. Nothing here is revised.
