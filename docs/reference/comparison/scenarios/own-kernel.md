# Own kernel

Record whether the tool can put the workload behind a kernel of its own, by any documented route.[^read]

1. Read the tool's own documentation for a mode that boots a guest kernel.
2. Start the workload through that mode.
3. Inside, run `uname -r` and record the value.
4. On the host, run `uname -r` and record the value.
5. Record every host prerequisite that mode required.

## flake-pilot

Yes, by two routes, and both are read in these tables. `firecracker-pilot` boots a firecracker microVM from a registered KIS image. `podman-pilot`, with podman's runtime set to `krun`, runs the registered container as a libkrun microVM, so the guest kernel is libkrunfw's rather than the image's. The route is fixed at registration rather than at run time, and upstream publishes a `claude` registration for each, which is why neither stands in for the other here.

## glaipnir

Yes: a libkrun microVM, gated by `_check_microvm` on `/usr/bin/krun`, `libkrun.so.1` above 1.18.0, `/dev/kvm`, and `kvm` group membership. `copilot` and `opencode` are held out of it unconditionally by a known libkrun vsock bug.

## bunkerbox

Yes, and it is the only mode: `ctr run --runtime io.containerd.kata.v2` starts the OCI image as a Kata Containers workload, so the guest kernel is Kata's rather than the image's and the image decides only the userland. Prerequisites are containerd, a Kata shim on `PATH`, `/dev/kvm`, and the `vhost_vsock` module, which the toolchain channel needs; without vsock the container still boots and passthrough is simply unavailable.

[^read]: Read at `flake-pilot` `main` on 2026-08-18, re-read 2026-08-20; `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
