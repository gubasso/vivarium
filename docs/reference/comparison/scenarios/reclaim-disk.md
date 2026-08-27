# Reclaim disk

Record how space is recovered from a sandbox that should keep working.[^read]

1. Fill the sandbox until it occupies noticeable disk.
2. Delete the data from inside.
3. Run the tool's reclamation and record what the host recovered.

## vivarium

Partial: what accumulates outside the guest is reclaimable today, and what accumulates inside a volume is not yet. `viv volume list` reports each volume's occupancy against its ceiling, `viv volume prune` removes exactly the orphans that list surfaces, and `viv gc` sweeps the store, reclaiming every retained build no generation root pins — all without a teardown. The scenario's own measure is the half that waits: a file deleted inside a volume frees space inside its sparse image and returns none of it to the host until `viv memory trim`, `viv volume trim`, and `viv volume rm` land — specified, not built, the `*`.

## flake-pilot

At the firecracker route, no: no reclamation verb. The overlay is a file of the declared `overlay_size`, `%remove` is a podman-only pseudo-argument, and `flake-ctl firecracker remove --vm` deletes the image together with every registration using it — a teardown, not a reclamation.

At the `krun` route, partial: `%remove` is one of the call-time pseudo-arguments the pilot consumes, and it removes the container the call created, so the writable layer that accumulated goes with it while the registration and the image both stay. What it does not reach is the image, which is the larger of the two and needs podman's own tooling; and reclaiming is a side effect of a call rather than a verb you can run against a machine, so there is nothing to ask what is worth removing.

## glaipnir

Yes, structurally: the container filesystem is disposable and everything durable is a host directory, so deleting a file inside frees host space immediately. `clean <agent>` and `clean <agent> all` cover the images. Holds at the microVM — krun changes the kernel, not where the bytes live.

## bunkerbox

No, and the cap is why. The overlay upper layer is a loopback ext4 image created at its quota before boot, so deleting a file inside frees space inside the image and returns none of it to the host; the same holds for the session image the persisted home uses. Neither shrinks, no verb asks either to, and the way space comes back is deleting `.bunkerbox/` — a teardown. The trade is deliberate and the other half of it is [the write-cap row](./host-write-cap.md#bunkerbox): a fixed-size image is what makes the ceiling real.

[^read]: Read at `vivarium` `e1b3c73` on 2026-08-26; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
