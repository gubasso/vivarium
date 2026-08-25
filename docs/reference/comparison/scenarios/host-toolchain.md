# Host toolchain

The guest ships one userland; the project may need another. Record whether a compiler the guest does not have can still be used from inside. Whether that costs the boundary is [the next row](./build-inside-boundary.md), and the two are kept apart on purpose: this one asks what the user gets, that one asks what it costs.[^read]

1. Take a project whose toolchain is absent from the guest image.
2. From inside, invoke that toolchain by its ordinary name.
3. Record whether it ran, where it ran, and what had to be declared for that to work.

## vivarium

No, by design: the [agent-channel allowlist](../../spec/08-invariants-and-guarantees.md) permits `ssh` and `gpg` and nothing else, so no guest process reaches a host program. The question is answered a layer down instead — the host store is shared read-only and the inner layer provisions its own store on top of it ([ADR-0084](../../../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md)), so a toolchain the host has already realised costs a mount rather than a rebuild, and it runs inside. That is a different capability from this row, and it is scored at [the inner-environment row](./inner-environment.md).

## flake-pilot

No, at either route. `sci` runs one command and reboots; the registration schema has no callback channel, and `force_vsock` at the firecracker route carries the pilot's own console rather than a command service.

## glaipnir

No: the whole CLI is `glaipnir.sh`, and packages reach the guest through the image's `PACKAGES` array at build time. A toolchain absent from the openSUSE base is added by rebuilding, not by reaching out.

## bunkerbox

Yes, and it is the tool's headline feature. The guest is a musl Alpine that deliberately ships no compilers; `bunkerbox-vscomm` reads the `passthrough` list from `.bunkerbox/project.conf`, symlinks every whitelisted command that is absent from the guest into `/usr/local/bunkerbox/bin/`, and prepends that directory to `PATH`. Invoking one proxies it over vsock port `9999` to a host daemon, which re-checks the whitelist, runs the real command in the overlay workspace, and streams stdout, stderr, and the exit code back. A command the guest already has wins, so baking `gcc` into an image takes it off the channel.

The list is filled in for you. On first run, or whenever it is empty, bunkerbox scans the repository root for build-system markers and pre-fills it — nine detectors, from `Cargo.toml` to `meson.build`:

```yaml
passthrough:
  - "make *"     # make with any arguments
  - "cargo *"    # cargo build, cargo test, cargo clippy...
  - "go *"       # go build, go test, go vet...
```

[^read]: Read at `bunkerbox` `b7f14f3` on 2026-08-25. The `vivarium`, `flake-pilot`, and `glaipnir` verdicts are absence claims derived from the inventories in [`../feature-sweep.md`](../feature-sweep.md) at the revisions [`../sources.md`](../sources.md) pins, not from a fresh read of those subjects.
