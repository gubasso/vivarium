# Networking and egress

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

Each VM receives isolated networking. Egress is open by default and this does not weaken the separate-kernel boundary. A manifest may switch to an allowlist mode, which places default-deny enforcement on the host and rejects denied traffic promptly rather than letting it time out.

An allowlist entry may be a name or a literal address or block, and the two take different enforcement paths. An address or CIDR entry is programmed into the filter directly, because nothing has to be resolved to know what it permits. A name entry cannot be, since packet filters see addresses: a vivarium-owned gating resolver bridges the two, resolving a permitted name, installing or refreshing the address policy, and only then releasing the answer to the guest. Expiry and address changes update the policy so stale answers do not become permanent capabilities. Everything not allowlisted stays blocked, whether a guest reaches for it by name or by address. The accepted entry forms are enumerated in [networking and egress](../reference/spec/05-networking-and-egress.md).

The contract is independent of the link topology. A direct user-space vhost-user uplink is an implementation experiment; tap plus an uplink is the pre-authorized fallback. Both must preserve per-VM isolation, ordering between rule installation and answer release, and fail-fast denial.

The implementation seams are selected: pinned util-linux `unshare` and `nsenter` for the namespace pair, pinned iproute2 `ip` for the tap, the `nftables` crate rendering the libnftables JSON schema through the pinned `nft` binary, and `hickory-proto` with vivarium's own serve loop for the gating resolver, landed under `src/net/`. The [slice 004 revisions](../plan/slices/004-enforce-egress-allowlist/README.md) carry the selection evidence.

Exact matching, DNS behavior, update rules, and proof fixtures live in [networking and egress](../reference/spec/05-networking-and-egress.md), governed by invariant N8 in the [invariants](../reference/spec/08-invariants-and-guarantees.md).

## Governing decisions

- [ADR-0007](../decisions/ADR-0007-default-open-egress.md) — fixes open egress as the default and the allowlist as opt-in.
- [ADR-0044](../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md) — puts enforcement on the host and rejects denied traffic rather than dropping it.
- [ADR-0064](../decisions/ADR-0064-egress-allowlist-enforcement-model.md) — fixes the gating resolver inside a per-VM network namespace, and leaves the uplink experiment open.

## Unresolved

- [Q-006](../plan/open-questions.md#q-006--can-a-user-space-vhost-user-uplink-drive-the-pinned-cloud-hypervisor-directly) owns the topology experiment. [Slice 004](../plan/slices/004-enforce-egress-allowlist/README.md) closes it.
