# ADR-0071: Agent forwarding is a typed channel over a second, host-initiated vsock port

## Context and Problem Statement

[`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md) names authentication-agent forwarding the primary channel for personal secrets, but [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md) still defers it, and no page says how a host Unix socket reaches a guest that has its own kernel. A shared filesystem cannot carry a live socket: the endpoint is a kernel object and a share conveys the inode, not the listener, so the guest sees a name with nothing behind it. The wiring has to be settled before the shipped `ssh-agent` piece is honest.

## Considered Options

- Share the host socket through the filesystem transport, or mount `${XDG_RUNTIME_DIR}`.
- Guest-initiated vsock, forwarded to a per-port host listener.
- A second, host-initiated vsock port carrying a pool of parked connections.

## Decision Outcome

Chosen option: a second host-initiated vsock port — the only option that both works and leaves ADR-0065's authorization argument intact.

The first fails on mechanism, not on taste. The second would require the per-port host listener [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md) says vivarium creates none of — ever — which is a leg [`ADR-0065-control-socket-wire-protocol.md`](./ADR-0065-control-socket-wire-protocol.md) rests on. Instead vivarium parks idle host-opened connections on a dedicated credential port; the guest proxy consumes one per client connection and vivarium refills the pool. Every byte flows over a connection the host opened, so both properties hold by construction rather than by policy.

The channel is a closed allowlist of two — `ssh` (`$SSH_AUTH_SOCK`) and `gpg` (the restricted extra socket only) — landing at fixed guest paths, declared through a typed `vivarium.credentials.agents` enum, off by default. The value carries no host path, so a shared piece may declare it without violating N11.

## Consequences

- Good: the already-shipped `ssh-agent` piece becomes honest, and key material never crosses the boundary.
- Good: no new host listener, no multiplexer, and no relaxed authorization.
- Bad: new protocol surface and new guest-agent code, plus a pool to size and refill.
- Bad: a raw byte relay loses OpenSSH's remote-client restrictions; this is documented, not fixed.

## Status

Implemented — the guest end and parked pool landed with slice 003 ([`crates/vivarium-guest-agent/src/credentials.rs`](../../crates/vivarium-guest-agent/src/credentials.rs), [`src/launch/credentials.rs`](../../src/launch/credentials.rs)), and the host hop that resolves and carries the declared socket landed with [slice 031](../plan/slices/031-a-declared-agent-channel-reaches-the-guest/README.md) ([`src/launch/agent_source.rs`](../../src/launch/agent_source.rs), [`src/cli/lifecycle.rs`](../../src/cli/lifecycle.rs)), demonstrated end to end by `workflow_23_agent_channel_relay` in [`tests/boot_workflows.rs`](../../tests/boot_workflows.rs).

Amends [`ADR-0016-guest-control-transport-and-exec-contract.md`](./ADR-0016-guest-control-transport-and-exec-contract.md) and [`ADR-0065-control-socket-wire-protocol.md`](./ADR-0065-control-socket-wire-protocol.md) — the transport now carries a second port beside the control port. Their session model, framing, and authorization are unchanged, and deliberately so: the parked-connection design exists precisely to keep ADR-0065's "the guest cannot originate connections" true.

Amends [`ADR-0020-mount-and-config-mirroring-schema.md`](./ADR-0020-mount-and-config-mirroring-schema.md) and [`ADR-0021-typed-launch-channel-options-in-pieces.md`](./ADR-0021-typed-launch-channel-options-in-pieces.md) — a mount `source` carries filesystem data only, never a socket or other special file; sockets are this ADR's typed channel instead.

Specified in [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md), [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md), [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md), and [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
