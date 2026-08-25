# Update

Record what causes the environment to change.[^read]

1. Run the tool, and record what identifies the environment.
2. Wait for upstream to move, run it again, and record the identifier.
3. Record what the user did to cause the change.

## vivarium

Specified in part: the [pure-build rule](../../spec/08-invariants-and-guarantees.md) plus the lockfile mean the environment does not move until a user moves it — that pinning runs today. `viv update`, the verb that moves it, is specified and not implemented — the `*`.

## flake-pilot

At the firecracker route, yes: the registration names a local rootfs and kernel, so nothing can move on its own, and a newer image takes `flake-ctl firecracker pull --force`. What holds still is the copy on this machine, which is a different question from whether a second person running the same command gets that same copy — [the same-definition row](./same-definition.md#flake-pilot) asks that one.

At the `krun` route, no: the registration names a `:latest` tag on a registry that rebuilds nightly, so what a fresh machine or a re-pulled image gets is whatever was published that day. Nothing in the registration pins a version, nothing reports which build is running, and the moment of change is the registry's rather than the user's — the inverse of what this row asks for. Which is [the same reason the definition does not repeat](./same-definition.md#flake-pilot).

## bunkerbox

Yes: the environment is an archive on disk, imported from a fixed path the runtime config names, and nothing re-checks anything upstream at start. It changes when a new package is installed, which is a decision somebody made. The same yes flake-pilot's firecracker route earns, and for the same reason — a local artifact does not move on its own.

[^read]: Read at `vivarium` `ceb0027` and `flake-pilot` `main` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
