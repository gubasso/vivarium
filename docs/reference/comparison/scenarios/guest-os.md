# Guest OS

Record which operating system runs inside, and whether the user can name a different one.

1. Read the tool's own definition format for a key naming a base system or image.
2. Set it to a second, unrelated distribution and rebuild.
3. Start the workload and record what `/etc/os-release` says inside.
4. Record what had to change on the host for that to work.

## vivarium

vivarium `ceb0027`, 2026-08-18. No, by rule: the guest is a NixOS system because vivarium [composes through the NixOS module system](../../spec/08-invariants-and-guarantees.md) rather than a bespoke merge engine, and another distribution would need a second composition engine. The manifest chooses the package set and configuration inside that system, not the system. Not planned.

## flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: the guest is whatever the registered image is, built by any means the user likes — KIWI, podman, mkosi, OBS, koji. `firecracker-pilot` takes a KIS image with its own kernel and rootfs. The cost is the other side of that freedom, and it is this comparison's reading rather than an upstream claim: the boundary is only as good as the image the registration names, and nothing in the registration attests to what is in it.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `image/Containerfile` builds from `registry.opensuse.org/opensuse/tumbleweed:latest`, layered with a published per-agent image. `PACKAGES=(...)` and hooks extend that system; nothing selects a different one short of editing the `Containerfile`.

## podman

podman 5.x, read 2026-08-19. Yes: any image runs, so `/etc/os-release` inside is the user's choice. Under krun the kernel is libkrunfw's regardless of image — the userland is chosen, the kernel is not.
