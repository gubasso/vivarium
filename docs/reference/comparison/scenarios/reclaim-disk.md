# Reclaim disk

Record how space is recovered from a sandbox that should keep working.[^read]

1. Fill the sandbox until it occupies noticeable disk.
2. Delete the data from inside.
3. Run the tool's reclamation and record what the host recovered.

## vivarium

Specified, mostly not built: `viv volume list` reports occupancy today and `viv volume prune` removes orphans; `viv trim`, `viv volume trim`, `viv volume rm`, and `viv gc`'s store sweep are specified and not implemented — the `*`. A loss today against `podman system prune` and `glaipnir clean all`.

## flake-pilot

At the firecracker route, no: no reclamation verb. The overlay is a file of the declared `overlay_size`, `%remove` is a podman-only pseudo-argument, and `flake-ctl firecracker remove --vm` deletes the image together with every registration using it — a teardown, not a reclamation.

At the `krun` route, partial: `%remove` is one of the call-time pseudo-arguments the pilot consumes, and it removes the container the call created, so the writable layer that accumulated goes with it while the registration and the image both stay. What it does not reach is the image, which is the larger of the two and needs podman's own tooling; and reclaiming is a side effect of a call rather than a verb you can run against a machine, so there is nothing to ask what is worth removing.

## glaipnir

Yes, structurally: the container filesystem is disposable and everything durable is a host directory, so deleting a file inside frees host space immediately. `clean <agent>` and `clean <agent> all` cover the images. Holds at the microVM — krun changes the kernel, not where the bytes live.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18.
