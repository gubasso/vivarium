# Build inside the boundary

The companion to [the row above](./host-toolchain.md). Reaching a host toolchain is a capability; this row asks what it costs. Record whether a command the agent runs can execute anywhere but inside the guest kernel's boundary.[^read]

1. Start the sandbox and, from inside, run the project's ordinary build command.
2. On the host, record which process tree it appeared in and under which kernel.
3. Record what confines it there, and what the tool's own documentation says that confinement is worth.

## vivarium

Yes: nothing the guest runs leaves it. The [separate-kernel rule](../../spec/08-invariants-and-guarantees.md) puts every guest process behind the hardware-virtualization boundary, and the [agent-channel allowlist](../../spec/08-invariants-and-guarantees.md) fixes the only two things that cross as `ssh` and `gpg` over a credential port. There is no command channel to hold open, so a build is a guest process or it does not happen.

## bunkerbox

No, by design, and the design is the point of [passthrough](./host-toolchain.md): a whitelisted command runs on the host, under the host kernel, in the host's process tree. Two layers stand between that and your machine, and both are opt-in.

The outer one is the whitelist, which is not a boundary so much as a name filter — it decides which programs may be proxied, not what they may do once they are.

The inner one is a bubblewrap sandbox built from the `profiles` list in `.bunkerbox/project.conf`: only the declared binaries are visible, only the declared paths exist, and `--unshare-net` is the network boundary rather than the proxy variables, which upstream calls compatibility hints and not the boundary. Five profiles ship, for `rust`, `make`, `go`, `node`, and `python`.

`profiles` is empty by default, and the guide that documents them states what that means without softening it: proxied does not mean safe, without a sandbox `cargo build` runs as you with your filesystem, home directory, network, and environment variables, and an agent that can run arbitrary cargo commands can run arbitrary code through `build.rs` or a proc macro.

Upstream also scopes the sandboxed case honestly: home-relative cache paths are deliberate writable carryover, and profile declarations are trusted host policy rather than a complete rogue-process capability model. A second default compounds it — `project.env` is `relaxed`, which forwards the guest's environment to the host command apart from `HOME`, `PATH`, `XDG_*`, and `BUNKERBOX_*`; `paranoid` is the mode upstream names for stopping the guest setting `LD_PRELOAD`, `RUSTFLAGS`, or `PYTHONPATH`.

The `†` is because bunkerbox is not missing a confinement it meant to have. It chose the trade: a guest small enough to stay immutable, and the host's real toolchain reachable from inside it.

[^read]: Read at `bunkerbox` `b7f14f3` on 2026-08-25, and at `vivarium` `162f230`. The unlinked `✅ yes` cells for `flake-pilot` and `glaipnir` are absence claims derived from the inventories in [`../feature-sweep.md`](../feature-sweep.md), not from a fresh read: neither tool has a host-command channel for a build to leave through.
