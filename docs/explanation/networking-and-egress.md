# Networking and egress

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

Each VM receives isolated networking. Egress is open by default and this does not weaken the separate-kernel boundary. A manifest may switch to an allowlist mode, which places default-deny enforcement on the host and rejects denied traffic promptly rather than letting it time out.

An allowlist entry may be a name or a literal address or block, and the two take different enforcement paths. An address or CIDR entry is programmed into the filter directly, because nothing has to be resolved to know what it permits. A name entry cannot be, since packet filters see addresses: a vivarium-owned gating resolver bridges the two, resolving a permitted name, installing or refreshing the address policy, and only then releasing the answer to the guest. Expiry and address changes update the policy so stale answers do not become permanent capabilities. Everything not allowlisted stays blocked, whether a guest reaches for it by name or by address. The accepted entry forms are enumerated in [networking and egress](../reference/spec/05-networking-and-egress.md).

The contract is independent of the link topology, and the topology is settled: tap plus an uplink, in both modes. The direct vhost-user experiment ran and succeeded mechanically — the pinned passt drives the pinned Cloud Hypervisor — but that path carries guest traffic from shared memory into a process and out through host sockets without ever crossing a netfilter hook, so the allowlist's in-namespace forward chain would have nothing to attach to; the [slice 004 revisions](../plan/slices/004-enforce-egress-allowlist/README.md) carry the experiment record. What ships: the launch wrapper holds a per-VM user+net namespace pair open with the pinned `unshare`, configures the tap and the forwarding sysctl inside it, and runs the VMM joined into the pair so it can open the tap by name; one unprivileged `pasta` per VM joins the pair's namespaces for its device while keeping its sockets on the host side, and maps one out-of-subnet address to the host's own resolver. Under allowlist mode the default-deny ruleset and the allowlist's literal destinations reach the kernel before any packet path exists, and the gating resolver runs inside the pair as the only DNS the guest is given; under open mode the filter and the resolver are absent, not inert, and the guest's nameserver is the uplink's DNS forward directly.

The implementation seams: pinned util-linux `unshare` and `nsenter` for the namespace pair, pinned iproute2 `ip` for the tap, the `nftables` crate rendering the libnftables JSON schema through the pinned `nft` binary, and `hickory-proto` with vivarium's own serve loop for the gating resolver, under `src/net/` with the process composition in `src/launch/`.

Exact matching, DNS behavior, update rules, and proof fixtures live in [networking and egress](../reference/spec/05-networking-and-egress.md), governed by invariant N8 in the [invariants](../reference/spec/08-invariants-and-guarantees.md).

## Governing decisions

- [ADR-0007](../decisions/ADR-0007-default-open-egress.md) — fixes open egress as the default and the allowlist as opt-in.
- [ADR-0044](../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md) — puts enforcement on the host and rejects denied traffic rather than dropping it.
- [ADR-0064](../decisions/ADR-0064-egress-allowlist-enforcement-model.md) — fixes the gating resolver inside a per-VM network namespace, and leaves the uplink experiment open.

## Unresolved

- [Q-022](../plan/open-questions.md#q-022--what-makes-workflow_05-separate-a-denial-from-an-absent-uplink) owns the acceptance fixture that separates a denial from an absent uplink. [Slice 004](../plan/slices/004-enforce-egress-allowlist/README.md) closes it.
