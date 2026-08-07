# Guest control and secrets

The guest control subsystem is implemented in one shared host/guest codec and a private guest-agent binary; [implementation status](../reference/implementation-status.md) separates deterministic evidence from the target-host lane that still requires a real boot.

Interactive commands cross a host-local control channel into an in-guest agent. Each connection begins with the current boot identity and uses length-delimited frames. One connection owns one exec or shell session, including standard streams, optional PTY allocation, terminal resize, signals, and final status. Invalid identity or framing fails closed.

On the host, the transport first completes the backend's hybrid-vsock preamble and consumes its acknowledgement. The current boot identity is injected on the guest kernel command line, recorded independently in private `boot.json`, and echoed before a ping or process request is accepted. Process ownership stays inside that connection: non-PTY sessions have separate streams and a process group, while a PTY owns the controlling terminal and guest job control. In the guest both listeners bind the wildcard context id, so each accepted connection's peer is checked against the host context id and a guest-local loopback peer is dropped before any protocol byte is read.

Secrets never enter a build, generated flake, log value, or mounted host session directory. Declared credential channels use a dedicated credential port on the same host-initiated transport as the control plane, so no second host listener exists beside the control socket. The host opens and parks bounded connections; the guest proxy accepts a local connection at a fixed guest socket path, consumes one parked connection per client, and relays bytes opaquely. The guest cannot initiate a host connection, and vivarium neither decrypts credentials nor owns decryption identities.

The credential setup byte is consumed before parking; every later byte is opaque. Four host-opened slots exist per declared id and each closed relay refills only its own slot. The guest creates only the declared fixed sockets under `/run/vivarium`, whose directory mode is `0700` and socket mode is `0600`, owned by `vivarium`. Command readiness waits for the current-boot ping and every initial declared slot.

The control and credential channels share a transport but not authority. Boot identity authorizes the control session inside one runtime lifetime, and the credential port carries no session semantics of its own. Confinement grants only the exact host socket connection and never exposes the runtime directory as a filesystem share.

Exact frames, environment rules, socket behavior, and secret constraints live in [secrets and config sharing](../reference/spec/07-secrets-and-config-sharing.md), [exec and shell](../reference/spec/12-exec-and-shell.md), [logging and diagnostics](../reference/spec/16-logging-and-diagnostics.md), and invariants N10/N17/N24/N25 in the [invariants](../reference/spec/08-invariants-and-guarantees.md).

## Governing decisions

- [ADR-0010](../decisions/ADR-0010-secrets-never-in-nix-store.md) — keeps secrets out of the Nix store, which is why credentials travel at runtime.
- [ADR-0016](../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) — fixes the host-local control transport and the exec contract over it.
- [ADR-0065](../decisions/ADR-0065-control-socket-wire-protocol.md) — fixes length-delimited framing and boot-identity authorization.
- [ADR-0069](../decisions/ADR-0069-redaction-is-by-construction.md) — makes redaction a property of the types rather than a scrubbing pass.
- [ADR-0071](../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — puts credential relay on a second host-initiated port instead of a second listener.
- [ADR-0072](../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md) — refuses to own decryption identities.
