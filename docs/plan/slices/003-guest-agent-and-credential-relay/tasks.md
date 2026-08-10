# 003 — Remaining work

The implementation landed; the evidence is now partly in. Everything below is what stands between this slice and its [`README.md`](./README.md) acceptance assertions. Status stays in [`../../milestones.md`](../../milestones.md); this file is deleted when that row flips to `done`.

## What the target-host lane now proves

`tests/host/guest-agent-check` builds the verification image, exports `VIVARIUM_AGENT_RUNNER`, and runs `tests/guest_agent_host.rs` twice. The trial is gated at run time and reports why it skipped, so an incapable host no longer panics on a missing variable. Observed on a real host with `/dev/kvm`, a systemd user manager, guest kernel 6.18.38 and cloud-hypervisor 52.0:

- The guest boots and the supervisor reports readiness in about 6.2 s; the first `Pong` follows a control connect in about 0.4 ms. Readiness ordering holds, because the runner exits zero only after the agent ping and the initial credential pool, and every later assertion connects without waiting.
- Cloud Hypervisor presents the host as context id `2`. This is implied rather than separately measured: the accept loop drops every other peer, so an answered frame is the observation.
- Exec sessions return their streams and status: `42`, `0`, and `143` from a signalled guest.
- A terminal session honours the pre-`Start` size, reporting `31 97`.
- A stale boot identity is refused with the `authorization` code, the connection closes, and a marker file proves the refused connection executed nothing.
- Eight concurrent sessions each holding two seconds complete in about 2.0 s, so they really are concurrent rather than serialized.
- The running `vivarium-agent` unit matches `nix/guest.nix`: `User`/`Group` `vivarium`, empty `CapabilityBoundingSet` and `AmbientCapabilities`, `NoNewPrivileges`, `Restart=on-failure`, `RuntimeDirectoryMode=0700`. That is also the observation that an unprivileged user with no capabilities binds both vsock ports.
- `/run/vivarium` is `0700` and `/run/vivarium/ssh-agent.sock` is `0600`, both owned by the guest user, and `SSH_AUTH_SOCK` is the guest path on both spawn paths despite a poisoned value in the request.
- Credential bytes relay opaquely for as many clients as the pool is deep.

## The blocker: the credential pool does not refill

This is the slice's own Core, and it fails. The pool serves `CREDENTIAL_POOL_SIZE` clients and then serves no more, for the life of the VM. Measured on a live guest: one client succeeded, then three of four, then none of six — consumption without replacement.

The guest's own view while wedged: `/run/vivarium/ssh-agent.sock` had fourteen connections queued in its accept backlog, four vsock connections on port `52001` sat `ESTAB` with empty queues, and `serve_local` was waiting on an empty channel.

The mechanism is a teardown deadlock. `serve_local` pairs a parked connection with a local client through `copy_bidirectional`. When the local client closes, that call shuts down the vsock write half — `tokio-vsock`'s `poll_shutdown` really does issue `shutdown(SHUT_WR)` — and then waits for the vsock read half to reach end of file. It never does: the host end of the hybrid connection stays open because its own `copy_bidirectional` is waiting for the same signal from the other direction. Neither side closes, the consumed connection is never released, and the host worker never re-establishes.

Deciding the fix needs one fact this lane has not yet established: whether the pinned hypervisor propagates a guest's `SHUT_WR` on a hybrid-vsock connection to a half-close on the host Unix socket. If it does, the defect is elsewhere and is probably small. If it does not, the relay cannot rely on half-close at all, and the guest has to end the relay on the local client's own closure — which needs a rule for how long an already-requested response may still arrive, because a client that half-closes after writing its request is legitimate.

Escapes to weigh once that is known: end the relay when the local side closes and bound the response drain the way `session.rs` bounds its exit drain; or have the host worker, rather than the guest, own the lifetime.

## Close the remaining host-lane gaps

Written and never reached, because the run stops at the relay:

- The guest-local `vsock_loopback` rejection on both ports, with its positive control. The module now ships in the verification image only (`tests/nix/default.nix`), and the check fails rather than skips if loopback does not work, so it cannot pass vacuously.
- `EXIT_DRAIN_LIMIT` and `DISCONNECT_GRACE` measured against a real guest. Both are still judgements. The measurements are implemented; neither has produced a figure.
- The agent's restart after a crash, which `Restart=on-failure` claims.

## Deferred

The live GPG relay variant stays cut. It is optional to the Core and is listed here so the cut stays visible rather than forgotten.
