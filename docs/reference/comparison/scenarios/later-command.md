# Later command

Two questions hide in "can I get back into it". This one asks whether the instance outlives the command that started it, so that the next invocation joins it rather than booting a fresh one; [the next](./concurrent-sessions.md) asks whether two can be inside at once.

1. Start the instance and let the first command finish.
2. Run a second command against the same instance.
3. Record whether it joined the existing instance or created another.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the VM outlives the command, and `start` is idempotent — on a fresh, already-running VM it is a no-op that exits `0`. That ensure-running step is the shared routine `viv exec` and `viv shell` reuse when they start the VM if needed, so joining and starting are the same code path reached from either state.

## flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: `--resume` keeps the instance, and with `--force-vsock` the VM stays alive host-side so the next call reaches it over the vsock rather than booting a second one. The registration is where that is fixed, and it is fixed for firecracker: upstream states the `krun` handler does not support `exec`, so a `krun` registration cannot use `--resume` either.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: a running krun container cannot be entered at all, so `run` cannot resume into one. The script counts what exists and starts a numbered sibling instead. The `podman exec` and `podman start -ai` resume path that `run` does have is the container backend's.

## podman

podman 5.x, read 2026-08-19. No: `podman exec` cannot enter a krun container — there is no in-guest agent to inject a process into. The container keeps running and remains listed; what cannot happen is getting back inside it. The exec that works is the shared-kernel runtime's.
