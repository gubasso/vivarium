# 013 — Exec and shell

## Goal

Connect `viv exec` and `viv shell` to the guest agent already proved on a real host, so that a command run inside the sandbox reports the result it actually produced.

## Appetite

2 implementation sessions.

## Core

`viv exec` runs a command in the booted guest and propagates its exit code, and `viv shell` opens an interactive PTY session that honors job control.

## In scope

Ordered, because a non-interactive exec is the smaller case and its failures are legible; the PTY case adds signal and sizing behavior on top of a path already known to work.

1. Wire `viv exec` from the command surface to the existing host control client in [`../../../../src/launch/control.rs`](../../../../src/launch/control.rs). The guest half and the wire protocol are done and host-proved; this is connection, not construction.
2. Map the guest process result onto the host process code, so an inner failure is distinguishable from a vivarium failure.
3. Wire `viv shell` onto the PTY session, including pre-`Start` sizing and job control.
4. Land the `Virtualization` trial this slice's `Acceptance` names, unskipped, and give `viv shell` the interactive coverage [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) currently records as absent.

## Out of scope

- Any change to the guest agent, the wire protocol, or the credential relay. All three are done and proved; a change here means a defect was found, and it is recorded as one.
- Agent forwarding behavior beyond confirming the declared second port still works after this slice's wiring.
- Volume persistence and workspace mounts, which are [slice 014](../014-workspace-and-persistence/README.md)'s.
- Egress restriction inside an exec session.

## Governed by

- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — defines the exec and shell contract.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — defines how an inner code differs from a vivarium code.
- [`../../../explanation/guest-control-and-secrets.md`](../../../explanation/guest-control-and-secrets.md) — owns the control topology this slice connects to.
- [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) — records the interactive-PTY coverage gap item 4 closes.
- [`../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md`](../../../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) — fixes the transport and the exec contract.
- [`../../../decisions/ADR-0065-control-socket-wire-protocol.md`](../../../decisions/ADR-0065-control-socket-wire-protocol.md) — fixes the framing this slice speaks.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes the second port the relay uses.

## Acceptance

When `viv exec` runs a command that fails inside the guest, the host process SHALL return the guest command's code and `workflow_06_exec_exit_code_propagation` SHALL pass unskipped.

Where a failure is vivarium's rather than the guest command's, the code returned SHALL be distinguishable from any code the guest command could have produced.

When `viv shell` opens a session, the guest SHALL receive the terminal size before the session starts, and job control SHALL work.

While the control socket is not yet ready, `viv exec` SHALL fail with a stated reason rather than hang.

## Rabbit holes

- Reopening protocol questions the guest agent already answered — escape: the guest half passes twice on a capable host; treat it as fixed and change the host side.
- Building a general terminal multiplexing layer — escape: one PTY session per invocation is the contract.
- Conflating an inner non-zero exit with a vivarium error — escape: item 2 exists precisely to keep the two apart, and the acceptance assertions test both directions.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, `viv shell` carries interactive coverage rather than usage-only coverage, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved to Implemented, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
