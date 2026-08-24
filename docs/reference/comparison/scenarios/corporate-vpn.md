# Corporate VPN

Record which host interface the tool's traffic leaves by.[^read]

1. Bring up a VPN on the host so it owns or shares the default route.
2. Start the tool and, from inside, reach an address that logs its source.
3. Record the observed source, and what the tool did about the VPN.

## vivarium

No, and the behavior is inheritance rather than absence: the uplink keeps its sockets on the host side and opens its device inside the VM's namespace pair, so guest flows are re-originated as host sockets and take the host routing table. A VPN owning the default route therefore carries the sandbox's traffic, and the uplink's DNS forward reaches the host's configured resolver, so name lookups go the same way. `allowlist` mode narrows which destinations may be reached without changing which path reaches them. No manifest key selects an interface, and the specification names neither the threat nor the inherited default; the scope question is [`Q-032`](../../../plan/open-questions.md).

## flake-pilot

At the firecracker route, partial, and it is the operator's plumbing rather than the tool's: firecracker reaches the network only through a TAP device the host owns, and the routing from it — `ip_forward`, a MASQUERADE rule, the interface that rule names — is set up by hand. Which interface the traffic leaves by is therefore decidable, and decidable only by someone who writes that rule themselves. Nothing in a registration names an interface, and the same hedge for the same reason is [the inbound row](./inbound.md#flake-pilot).

At the `krun` route, no, and nobody gets to decide. Both of libkrun's networking modes make the guest's outbound connections as ordinary sockets in the host's own network namespace, so both follow the host routing table and a VPN that owns the default route owns them. Under transparent socket impersonation, which is the mode the upstream registration's `--net host` selects, "the VMM acts as a proxy for AF_INET, AF_INET6 and AF_UNIX sockets, for both incoming and outgoing connections" — and the VMM is a host process holding host sockets. Under the `krun.use_passt` alternative, passt implements "a translation layer between a Layer-2 virtual network interface and native Layer-4 (TCP, UDP, ping) sockets on the host", selects a source address by the host routing tables by default, and takes its gateway from "the host interface with the first default route".

passt does have `--outbound-if4` and `--outbound-if6` to bind outbound sockets to a named interface, which is exactly the decision the firecracker route's hand-written MASQUERADE rule makes. Nothing reaches it: `crun`'s `krun` handler exposes a fixed set of scalar annotations — `krun.cpus`, `krun.ram_mib`, `krun.gpu_flags`, `krun.use_passt`, `krun.nested_virt`, `krun.variant` — and `krun.use_passt` is an on-or-off switch with no argv behind it. The stronger boundary is the one with the weaker answer here, because the boundary is the kernel's and the route out is still the host's.

## glaipnir

Yes: `_detect_public_iface` reads the default route, filters out `tun|wg|vpn|tap|ppp|openvpn|docker0|br-`, and binds egress with `--network pasta:--outbound-if4,<iface>`. No qualifying interface aborts the run — the one place the script is fatal rather than degrading.

## podman

Reachable, nothing arranges it: the mechanism glaipnir uses is podman's own — `--network pasta:--outbound-if4,<iface>` binds egress to a named interface, and it is available to anyone typing the flag. What podman does not do is what glaipnir does around it: nothing reads the default route, nothing filters `tun` or `wg` out of the answer, and a run that omits the flag takes the host's routing table with the VPN on it.

[^read]: Read at `vivarium` `ceb0027`, re-read 2026-08-19; `glaipnir` `21ef389` on 2026-08-18; `flake-pilot` `920f41e` and `podman` 5.x on 2026-08-20.
