# ADR-0065: The control socket speaks length-delimited frames and authorizes by boot identity

## Context and Problem Statement

[`ADR-0016-guest-control-transport-and-exec-contract.md`](./ADR-0016-guest-control-transport-and-exec-contract.md) left the wire framing and the auth handshake as future work, and [`ADR-0032-cli-dependency-baseline.md`](./ADR-0032-cli-dependency-baseline.md) deferred the vsock-class transport crate to this decision. `spec/12` has since settled the session model — a listening socket, one connection per `exec`/`shell`, no multiplexer and no registry — so what remains is the single-session protocol and what "authenticated ping" means.

## Considered Options

- An RPC framework (protobuf/ttrpc) over the channel — the shape a multi-container agent uses.
- A shared per-boot secret with a challenge/response handshake.
- Length-delimited frames plus authorization by socket permissions and boot identity.

## Decision Outcome

Chosen option: length-delimited frames, authorized by boot identity — the session model already removed the problem an RPC framework exists to solve, and a shared secret protects against no adversary the runtime directory does not already exclude.

- The transport is the backend's hybrid vsock: the VMM listens on a host Unix socket and a connection is completed to a guest port. The host end is therefore an ordinary Unix stream and the CLI needs no vsock crate at all; the vsock dependency belongs to the guest agent.
- Frames are a length prefix, a type tag, and a payload — raw bytes for standard I/O, serde-encoded structures for control. Unknown tags are protocol errors, and frame direction is part of the contract.
- "Authenticated" is redefined, not elaborated: the session-scoped `0700` runtime directory excludes other host users, the guest cannot originate connections, and the handshake's job is to prove the peer is this boot's agent by echoing the identity `boot.json` records.

## Consequences

- Good: ADR-0032's deferral is discharged without adding a build-time toolchain.
- Good: no secret has to be delivered into the guest, so no new secret-handling path exists.
- Bad: `spec/12`'s "authenticated ping" and ADR-0016's wording change meaning; a reader who expected a shared secret must be told why there is none.
- Bad: the CLI and the guest agent now depend on a hand-written protocol whose compatibility is vivarium's to keep.

## Status

Accepted

Extends [`ADR-0016-guest-control-transport-and-exec-contract.md`](./ADR-0016-guest-control-transport-and-exec-contract.md) — its deferred framing and handshake are supplied here, and its phrase "authenticated control-socket ping" now means boot-identity attestation rather than a shared secret. The session model is unchanged.

Amends [`ADR-0032-cli-dependency-baseline.md`](./ADR-0032-cli-dependency-baseline.md) — the deferred vsock-class transport crate resolves to none in the CLI; the guest agent's crate is chosen with the agent.

Amended by [`ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](./ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — the transport carries a second guest port for the agent channel, with its own minimal protocol. The framing, session model, and authorization above are untouched, and the parked-connection design was chosen precisely so that the third authorization leg here — "the guest cannot originate connections" — stays literally true.

Specified in [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md).
