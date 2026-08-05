# Guest control and secrets

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

Interactive commands cross a host-local control channel into an in-guest agent. Each connection begins with the current boot identity and uses length-delimited frames. One connection owns one exec or shell session, including standard streams, optional PTY allocation, terminal resize, signals, and final status. Invalid identity or framing fails closed.

Secrets never enter a build, generated flake, log value, or mounted host session directory. Declared credential channels use a dedicated credential port on the same host-initiated transport as the control plane, so no second host listener exists beside the control socket. The host opens and parks bounded connections; the guest proxy accepts a local connection at a fixed guest socket path, consumes one parked connection per client, and relays bytes opaquely. The guest cannot initiate a host connection, and vivarium neither decrypts credentials nor owns decryption identities.

The control and credential channels share a transport but not authority. Boot identity authorizes the control session inside one runtime lifetime, and the credential port carries no session semantics of its own. Confinement grants only the exact host socket connection and never exposes the runtime directory as a filesystem share.

Exact frames, environment rules, socket behavior, and secret constraints live in [secrets and config sharing](../reference/spec/07-secrets-and-config-sharing.md), [exec and shell](../reference/spec/12-exec-and-shell.md), [logging and diagnostics](../reference/spec/16-logging-and-diagnostics.md), and invariants N10/N17/N24/N25 in the [invariants](../reference/spec/08-invariants-and-guarantees.md).

## Governing decisions

- [ADR-0010](../decisions/ADR-0010-secrets-never-in-nix-store.md) — keeps secrets out of the Nix store, which is why credentials travel at runtime.
- [ADR-0016](../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) — fixes the host-local control transport and the exec contract over it.
- [ADR-0065](../decisions/ADR-0065-control-socket-wire-protocol.md) — fixes length-delimited framing and boot-identity authorization.
- [ADR-0069](../decisions/ADR-0069-redaction-is-by-construction.md) — makes redaction a property of the types rather than a scrubbing pass.
- [ADR-0071](../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — puts credential relay on a second host-initiated port instead of a second listener.
- [ADR-0072](../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md) — refuses to own decryption identities.

## Unresolved

- The in-guest endpoint and credential relay belong to [slice 003](../plan/slices/003-guest-agent-and-credential-relay/README.md).
