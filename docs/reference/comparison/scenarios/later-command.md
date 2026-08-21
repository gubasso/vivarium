# Later command

Two questions hide in "can I get back into it". This one asks whether the instance outlives the command that started it, so that the next invocation joins it rather than booting a fresh one; [the next](./concurrent-sessions.md) asks whether two can be inside at once.[^read]

1. Start the instance and let the first command finish.
2. Run a second command against the same instance.
3. Record whether it joined the existing instance or created another.

## vivarium

Yes: the VM outlives the command, and `start` is idempotent — on a fresh, already-running VM it is a no-op that exits `0`. That ensure-running step is the shared routine `viv exec` and `viv shell` reuse when they start the VM if needed, so joining and starting are the same code path reached from either state.

## flake-pilot

At the firecracker route, yes: `--resume` keeps the instance, and with `--force-vsock` the VM stays alive host-side so the next call reaches it over the vsock rather than booting a second one.

At the `krun` route, no, and upstream says so in the note that publishes the registration. The `krun` OCI handler "does not support the exec command", because "libkrun runs workloads inside isolated microVMs, and there is no built-in mechanism or agent inside the lightweight virtual machine to spawn and inject new secondary processes", and "because of this a `krun` based app registration cannot use the resume feature". The upstream `krun` registration accordingly carries no `--resume`, and without it `podman-pilot` points the container's entry point at the registered target rather than at a sleep, so the container ends when the command does. The next call creates another.

The route with the weaker in-guest story is the one that wins this row, and the reason is symmetrical: firecracker keeps a VM alive and talks to it over a vsock channel flake-pilot built, while `krun` inherits podman's re-entry model and podman's re-entry model is `exec`.

## glaipnir

No: a running krun container cannot be entered at all, so `run` cannot resume into one. The script counts what exists and starts a numbered sibling instead. The `podman exec` and `podman start -ai` resume path that `run` does have is the container backend's.

## podman

No: `podman exec` cannot enter a krun container — there is no in-guest agent to inject a process into. The container keeps running and remains listed; what cannot happen is getting back inside it. The exec that works is the shared-kernel runtime's.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
