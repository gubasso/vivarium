# Corporate VPN

Record which host interface the tool's traffic leaves by.[^read]

1. Bring up a VPN on the host so it owns or shares the default route.
2. Start the tool and, from inside, reach an address that logs its source.
3. Record the observed source, and what the tool did about the VPN.

## vivarium

No, and the behavior is inheritance rather than absence: the uplink keeps its sockets on the host side and opens its device inside the VM's namespace pair, so guest flows are re-originated as host sockets and take the host routing table. A VPN owning the default route therefore carries the sandbox's traffic, and the uplink's DNS forward reaches the host's configured resolver, so name lookups go the same way. `allowlist` mode narrows which destinations may be reached without changing which path reaches them. No manifest key selects an interface, and the specification names neither the threat nor the inherited default; the scope question is [`Q-032`](../../../plan/open-questions.md).

## glaipnir

Yes: `_detect_public_iface` reads the default route, filters out `tun|wg|vpn|tap|ppp|openvpn|docker0|br-`, and binds egress with `--network pasta:--outbound-if4,<iface>`. No qualifying interface aborts the run — the one place the script is fatal rather than degrading.

[^read]: Read at `vivarium` `ceb0027`, re-read 2026-08-19; `glaipnir` `21ef389` on 2026-08-18.
