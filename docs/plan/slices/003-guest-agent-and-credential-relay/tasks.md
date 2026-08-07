# 003 — Remaining work

The implementation landed; the evidence did not. Everything below is what stands between this slice and its [`README.md`](./README.md) acceptance assertions. Status stays in [`../../milestones.md`](../../milestones.md); this file is deleted when that row flips to `done`.

## Run the target-host lane

`tests/guest_agent_host.rs` carries `guest_agent_and_credential_relay_on_capable_host`, written but never executed. It is `#[ignore]`d because it needs Linux, Nix, `/dev/kvm`, a systemd user manager, and `XDG_RUNTIME_DIR`. Run it on a host that has them:

```bash
cargo test --test guest_agent_host -- --ignored
```

Run it twice. One clean run is evidence of possibility, not reliability.

## Confirm the host premises

Each of these is currently a reasoned inference, not an observation. None can be settled in an agent environment, per the target-host rule in [`../../../../AGENTS.md`](../../../../AGENTS.md).

- Cloud Hypervisor presents context id `2` to the guest. Verified against v52.0 `virtio-devices/src/vsock/mod.rs`, never observed live. Both accept loops reject any other peer, so if this is wrong the agent rejects every host connection.
- Real boot and hybrid-vsock acknowledgement timing, including the `OK <port>` line the host consumes before its first frame.
- Unprivileged `AF_VSOCK` binding on ports `52000` and `52001`.
- The guest service's runtime user, socket ownership and mode, restart behavior, and capability restrictions.
- Guest PTY job control: resize, EOF, interrupt, teardown.
- End-to-end credential opacity, pool depletion and refill, readiness ordering, repeated relay use.
- Real guest socket ownership and the fixed `SSH_AUTH_SOCK`.

## Close the deterministic gaps

These do not need a capable host and can land before it:

- Post-start PTY resize, and PTY interrupt behavior.
- Non-UTF-8 process execution.
- Explicit stdin end-of-input.
- Process-group signaling driven through a full control session.
- Concurrent-session races beyond the single case covered.
- Exact golden encoding for every message tag.
- Credential refill races at the 20-iteration repetition the rest of the suite uses.
- Explicit signal support currently covers common Unix signals rather than every valid numeric signal.

## Close the host-lane gaps

Assertions the host test does not yet make: stale identity rejected without executing anything, exit codes `0` and `143`, concurrent live sessions, socket-ownership inspection, and direct verification of `SSH_AUTH_SOCK`.

A guest-local loopback peer is rejected on both vsock ports by the peer-origin check, and nothing proves it. The test needs real `AF_VSOCK` plus the `vsock_loopback` kernel module, and passes vacuously wherever that module is absent — inert is not absent. It belongs here, not in the deterministic suite.

## Measure the two time bounds

`EXIT_DRAIN_LIMIT` and `DISCONNECT_GRACE` in `crates/vivarium-guest-agent/src/session.rs` are judgments. Neither has been exercised against a real guest. Replace them with measured figures, or record why the judgment holds.

## Deferred

Phase 5's 20-iteration stress lane and the live GPG relay variant were cut for want of a capable host. They are optional to the Core and are listed here so the cut stays visible rather than forgotten.
