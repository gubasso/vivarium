# ADR-0064: Enforce the egress allowlist with a gating resolver in a per-VM network namespace

## Context and Problem Statement

[`ADR-0044-host-side-egress-and-reject-not-drop.md`](./ADR-0044-host-side-egress-and-reject-not-drop.md) fixed _where_ enforcement sits (host-side) and _how_ a denial reads (reject, never drop), but not the mechanism. Left open: the connectivity substrate, the allowlist grammar, DNS, IPv6, and what a denial concretely is on the wire. Without these the enforcement crates cannot be written and `workflow_05_restrict_egress_allowlist` cannot falsify anything.

## Considered Options

- **User-mode networking with an in-hypervisor filter** — no host network setup.
- **Resolve the allowlist once at start into an address set** — a static firewall, no runtime resolver.
- **Per-VM network namespace holding a tap, a default-deny ruleset, and a gating resolver.**

## Decision Outcome

Chosen option: **per-VM network namespace with a gating resolver** — it is the only option that survives an adversarial guest _and_ a name whose addresses move.

- The N20 launch wrapper already creates a user+network namespace pair per (project, target). Inside it, it holds `CAP_NET_ADMIN` and needs no host root, no host network state, and no new prerequisite: `host-userns-available` is already a hard doctor check.
- The guest's only resolver is vivarium's, inside that namespace. On an allowed query the resolver programs the returned addresses into the ruleset **before** writing the reply, with the TTL as the element timeout. Programming after the reply is a race, not an optimization.
- Denials are legible by mechanism: `REFUSED` for a denied name, TCP reset and ICMP administratively-prohibited for traffic.
- The grammar stays an array of strings — no new key shape, no port axis.

## Consequences

- Good: the ruleset lives where the guest has no handle on it; a moving CDN address stays enforceable.
- Good: no host daemon, no host root, no host network configuration.
- Bad: vivarium now ships a resolver, which is a component it must maintain and secure.
- Bad: `spec/05`'s user-mode-networking paragraph was wrong for the shipped backend and is replaced.

## Status

Accepted

Extends [`ADR-0044-host-side-egress-and-reject-not-drop.md`](./ADR-0044-host-side-egress-and-reject-not-drop.md), which is unchanged — this supplies its mechanism.

Amends [`ADR-0007-default-open-egress.md`](./ADR-0007-default-open-egress.md) — the allowlist mechanism it left abstract is now fixed; the open-by-default decision is untouched.

Specified in [`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md), with the allowlist string grammar in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) and the resolver's tie to `egress-allowlist-dns` in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).

The **uplink topology inside the namespace is deliberately unspecified** — the ADR fixes the property (one unprivileged host-side uplink process per VM), not the wiring, so a Part II experiment can collapse tap-plus-uplink into a single vhost-user process without touching a specified sentence. The pieces are known to be compatible in principle: the vhost-user net backend requires shared guest memory, which ADR-0035 already mandates. Whether a given user-space uplink drives the Cloud Hypervisor backend is **unverified** — upstream documents native support for qemu only.
