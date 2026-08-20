# Own kernel

Record whether the tool can put the workload behind a kernel of its own, by any documented route.[^read]

1. Read the tool's own documentation for a mode that boots a guest kernel.
2. Start the workload through that mode.
3. Inside, run `uname -r` and record the value.
4. On the host, run `uname -r` and record the value.
5. Record every host prerequisite that mode required.

## flake-pilot

Yes, two routes: `podman-pilot` with podman's runtime set to `krun`, and `firecracker-pilot`. The route is fixed at registration, not at run time. The compared setup is the upstream `claude` firecracker registration.

## glaipnir

Yes: a libkrun microVM, gated by `_check_microvm` on `/usr/bin/krun`, `libkrun.so.1` above 1.18.0, `/dev/kvm`, and `kvm` group membership. `copilot` and `opencode` are held out of it unconditionally by a known libkrun vsock bug.

## podman

Yes: `--runtime krun` runs the container as a libkrun microVM, with libkrun installed and `/dev/kvm` accessible as prerequisites. The guest kernel comes from libkrunfw, not from the image — the runtime decides the kernel, the image decides the userland.

[^read]: Read at `flake-pilot` `main` on 2026-08-18, re-read 2026-08-20; `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
