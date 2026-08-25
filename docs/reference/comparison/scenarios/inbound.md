# Inbound

Record whether a process outside the sandbox can connect to a service running inside it.[^read]

1. Start a listener inside on a known port.
2. From the host, attempt to connect to it.
3. Record the result and what configuration, if any, was needed.

## vivarium

No, and nothing is arranged either way: the guest's network sits in its own namespace behind an uplink the launcher starts with no port-forwarding argument, so there is no host-visible address, and `exec` and `shell` reach the guest over the vsock control plane rather than a listening socket, so there is no key to reach one with. `spec/05` describes egress and is silent on the other direction, and `spec/00` does not list inbound reachability as a non-goal. An editor over SSH, a guest dev server in the host browser, and a hosted notebook have no answer today; the scope question is [`Q-033`](../../../plan/open-questions.md), which [slice 023](../../../plan/slices/023-a-declared-port-crosses-inward/README.md) is shaped to answer by building the declaration. The verdict stays a bare no rather than a `*` until it does: nothing in the specification closes the direction yet, and a shaped slice is not a contract.

## flake-pilot

At the firecracker route, partial: reachable but entirely the operator's job — a TAP device per instance (`@NAME` names it), `ip_forward`, MASQUERADE, hand-edited `boot_args`. Nothing in the tool arranges any of it, and that is host plumbing the user builds rather than a mechanism the tool offers, which is why this stays a hedge rather than becoming a qualified yes.

At the `krun` route, yes, and the tool has a pseudo-argument for it. libkrun's transparent socket impersonation is on whenever no virtual interface is added, which is the case at the upstream registration, and an impersonated listener belongs host-side to the VMM process, so podman's ordinary publish path reaches it. Publishing is a `--opt "\-p ..."` line in the registration, and `%port:number` is in the set of call-time pseudo-arguments the pilot consumes, so a port can also be named at the call site without re-registering. The two bounds that hold at the engine hold here too: a guest cannot listen on datagram sockets, and adding `krun.use_passt` swaps impersonation for a virtio-net interface that no podman flag then wires a published port into.

## glaipnir

No: the assembled `podman run` publishes no port and there is no passthrough argument for adding one. The network flags the script sets are about egress.

## bunkerbox

No: nothing publishes a port. The container is started with `--cni` or `--net-host` and no port mapping, and no configuration key adds one. The two channels that do reach into the guest are vsock and belong to the tool — port `9999` carries the toolchain proxy, port `10000` the status client — so what crosses inbound is bunkerbox's own protocol rather than an arbitrary connection.

[^read]: Read at `vivarium` `ceb0027`, re-read 2026-08-19; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
