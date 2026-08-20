# Inbound

Record whether a process outside the sandbox can connect to a service running inside it.[^read]

1. Start a listener inside on a known port.
2. From the host, attempt to connect to it.
3. Record the result and what configuration, if any, was needed.

## vivarium

No, and nothing is arranged either way: the guest's network sits in its own namespace behind an uplink the launcher starts with no port-forwarding argument, so there is no host-visible address, and `exec` and `shell` reach the guest over the vsock control plane rather than a listening socket, so there is no key to reach one with. `spec/05` describes egress and is silent on the other direction, and `spec/00` does not list inbound reachability as a non-goal. An editor over SSH, a guest dev server in the host browser, and a hosted notebook have no answer today; the scope question is [`Q-033`](../../../plan/open-questions.md), which [slice 023](../../../plan/slices/023-a-declared-port-crosses-inward/README.md) is shaped to answer by building the declaration. The verdict stays a bare no rather than a `*` until it does: nothing in the specification closes the direction yet, and a shaped slice is not a contract.

## flake-pilot

Partial: reachable but entirely the operator's job — a TAP device per instance (`@NAME` names it), `ip_forward`, MASQUERADE, hand-edited `boot_args`. Nothing in the tool arranges any of it, and that is host plumbing the user builds rather than a mechanism the tool offers, which is why this stays a hedge rather than becoming a qualified yes.

## glaipnir

No: the assembled `podman run` publishes no port and there is no passthrough argument for adding one. The network flags the script sets are about egress.

## podman

Yes: `-p` publishes and the guest listener is reached through it. libkrun's transparent socket impersonation is on whenever no virtual interface is added, which is the default under podman, and the project states that applications in the VM receive connections from the outside to ports listening inside it. An impersonated listener belongs host-side to the VMM process, so it lands in the container network namespace podman's ordinary publish path already forwards into, and `-p` needs no krun-specific handling. A published run of `podman run --annotation=run.oci.handler=krun -dp 8080:8080` load-tested from the host is the demonstration. Two conditions bound it: a guest cannot listen on datagram sockets, and the `krun.use_passt` annotation swaps impersonation for a virtio-net interface that no podman flag then wires a published port into. A 2025 podman report has `krun` declining an `AF_INET6` listener, which is where a program binding `::` instead of `0.0.0.0` lands; upstream documents both families as supported, so that one is reported rather than settled.

[^read]: Read at `vivarium` `ceb0027`, re-read 2026-08-19; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x at `--runtime krun`, re-read 2026-08-19 from upstream documentation rather than run.
