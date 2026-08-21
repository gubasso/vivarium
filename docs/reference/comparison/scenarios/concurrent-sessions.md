# Concurrent sessions

The companion to [the row above](./later-command.md): not one session after another, but two at once, sharing one guest, one working tree, and one set of processes.[^read]

1. Start the instance and leave a process running inside it.
2. From a second terminal, open another session against the same instance.
3. Record whether it joined the existing instance or created another, and what the two sessions share.

## vivarium

Yes: `control.sock` is a listening socket, and each `viv exec` and each `viv shell` opens its own connection to it and performs the transport's per-connection session handshake. Session state — the PTY, the argv, the environment, the exit status — is per connection, and the multiplexing is the transport's rather than vivarium's. N terminals therefore share one guest rather than getting N guests, and an interactive session is a PTY sized before it starts, with job control and resize forwarding.

## flake-pilot

No at both routes, and neither has a supervisor inside to hand out a second shell.

At the firecracker route the guest init is `sci`, which executes the one command named by `run=` and reboots, so nothing is left inside to hand out a second one. Two concurrent sessions are therefore two VMs, separated by a call-time suffix that for firecracker also names the TAP device:

```bash
claude @projA        # one VM: instance projA, tap-claude@projA
claude @projB        # a second VM: its own overlay, memory, and TAP device
```

Two VMs is not the same capability as two shells: the sessions share no working tree, no running process, and no warm state. Registering the app as a multiplexer or an `sshd` would buy that back, at the cost of building the in-guest supervisor flake-pilot does not ship.

At the `krun` route the answer is the same and the reason is the engine's. `podman exec` is what would open a second session and the `krun` handler does not support it, for want of an in-guest agent to inject a process. `flake-ctl podman register` does carry an `--attach` flag, which is a different code path — `podman attach` rather than `podman exec` — and it is not this capability: attaching connects a second terminal to the primary process's existing stream rather than starting a session of its own, and podman's own report of two clients on one container is that output reaches only one of them. It is also mutually exclusive with `--resume` in the registration parser, and the upstream `krun` registration carries neither.

## glaipnir

No, and for the same reason it fails [the row above](./later-command.md#glaipnir): a krun guest cannot be entered even once more, so it cannot be entered twice. Two runs are two numbered sibling containers.

## podman

No: with no in-guest agent there is no way to inject a first extra process, let alone a concurrent one. Two `podman run` invocations are two microVMs with two rootfs layers.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
