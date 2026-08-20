# Reclaim disk

Record how space is recovered from a sandbox that should keep working.

1. Fill the sandbox until it occupies noticeable disk.
2. Delete the data from inside.
3. Run the tool's reclamation and record what the host recovered.

## vivarium

vivarium `ceb0027`, 2026-08-18. Specified, mostly not built: `viv volume list` reports occupancy today and `viv volume prune` removes orphans; `viv trim`, `viv volume trim`, `viv volume rm`, and `viv gc`'s store sweep are specified and not implemented — the `*`. A loss today against `podman system prune` and `glaipnir clean all`.

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: no reclamation verb. The overlay is a file of the declared `overlay_size`, `%remove` is a podman-only pseudo-argument, and `flake-ctl firecracker remove --vm` deletes the image together with every registration using it — a teardown, not a reclamation.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes, structurally: the container filesystem is disposable and everything durable is a host directory, so deleting a file inside frees host space immediately. `clean <agent>` and `clean <agent> all` cover the images. Holds at the microVM — krun changes the kernel, not where the bytes live.
