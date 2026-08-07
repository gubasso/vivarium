# 003 — Guest agent and credential relay

## Goal

Provide the in-guest endpoint for exec and shell sessions and declared credential forwarding.

## Appetite

3 implementation sessions.

## Core

Authenticated length-delimited control sessions allocate PTYs and streams correctly, and credential bytes relay only over host-initiated parked connections to guest-user-owned sockets.

## In scope

- Add the guest crate, vsock listener, framing codec, and boot-identity handshake.
- Implement PTY allocation, resize, signal, stream, and process-status handling.
- Implement the credential port, guest socket mode and owner, and bounded pool refill.
- Integrate the agent into the base image and command-readiness boundary.

## Out of scope

- Generic host socket mounting.
- Guest-initiated credential connections.
- Cryptography.
- Additional control protocols.

## Governed by

- [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md) — defines secret and credential boundaries.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — defines sessions and wire frames.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — defines N10, N17, N24, and N25.
- [`../../../explanation/guest-control-and-secrets.md`](../../../explanation/guest-control-and-secrets.md) — owns the subsystem topology.
- [`../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md`](../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) — fixes the control transport.
- [`../../../decisions/ADR-0065-control-socket-wire-protocol.md`](../../../decisions/ADR-0065-control-socket-wire-protocol.md) — fixes framing and boot identity.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes host-initiated credential relay.
- [`../../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md`](../../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md) — bounds cryptographic responsibility.

## Acceptance

When a host session presents the current boot identity, the guest agent SHALL accept framed exec and shell requests and SHALL return their streams and final status.

If identity or framing is invalid, then the guest agent SHALL fail closed.

Where a credential channel is declared, the guest proxy SHALL accept connections only from the guest user, and the guest SHALL NOT originate a host connection.

When the guest boots, the guest agent SHALL be available before command readiness is reported.

## Rabbit holes

- Hand-written raw `AF_VSOCK` — escape: use `tokio-vsock`.
- Credential pool depletion — escape: use bounded refill without guest initiation.
- Protocol expansion — escape: implement only the existing specification frames.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
