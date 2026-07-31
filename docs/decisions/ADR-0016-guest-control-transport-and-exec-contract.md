# ADR-0016: Guest control transport and exec contract

## Context and Problem Statement

`exec` and `shell` need a control path into an already-running or cold-started guest without depending on guest network egress. Scripts also need a clear boundary between vivarium failures and guest exit statuses.

## Considered Options

- In-guest agent over a vsock-class host-local transport.
- Network service in the guest.
- PID-file/process inspection plus ad-hoc command launch.

## Decision Outcome

Chosen option: **in-guest agent over a vsock-class host-local transport**.

vivarium uses a small in-guest agent over a vsock-class, host-local, network-independent transport, multiplexed through a host Unix control socket in the per-project runtime directory. Running VMs are detected by authenticated control-socket ping plus boot metadata under a per-project `flock`; PID files are diagnostic only. Exit codes use the guest-process-start boundary: before start, vivarium uses BSD sysexits; after start, it propagates the guest status verbatim.

## Consequences

- Good: works with default-deny egress and keeps the VM lifecycle idempotent under concurrency.
- Good: control behavior stays backend-agnostic and matches invariant N2.
- Bad: requires a small guest agent and a future protocol spec for framing/auth/multiplexing.

## Status

Accepted

Specified in [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md). The per-project `flock` and control-socket paths are keyed by the project-identity key ([`ADR-0029-project-identity-and-marker.md`](./ADR-0029-project-identity-and-marker.md)).

Amended by [`ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](./ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — the transport now carries a second guest port beside the control port, for the authentication-agent channel. The control contract above is unchanged: the credential port is a separate port with a separate protocol, and vivarium still originates every connection.

Amended by [`ADR-0065-control-socket-wire-protocol.md`](./ADR-0065-control-socket-wire-protocol.md) — the deferred wire framing and auth handshake are supplied there, and this ADR's phrase **"authenticated control-socket ping"** now means boot-identity attestation over a `0700`-scoped socket, not a shared secret. A reader expecting a secret here will not find one, and that is the decision. The transport and session model above are unchanged.
