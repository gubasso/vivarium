# Update

Record what causes the environment to change.[^read]

1. Run the tool, and record what identifies the environment.
2. Wait for upstream to move, run it again, and record the identifier.
3. Record what the user did to cause the change.

## vivarium

Specified in part: the [pure-build rule](../../spec/08-invariants-and-guarantees.md) plus the lockfile mean the environment does not move until a user moves it — that pinning runs today. `viv update`, the verb that moves it, is specified and not implemented — the `*`.

## flake-pilot

Yes: the registration names a local rootfs and kernel, so nothing can move on its own, and a newer image takes `flake-ctl firecracker pull --force`. The `:latest` tag rebuilt daily is the container backend — which is why this row is read at the boundary.

## podman

Partial: a pulled image stays until something pulls again, but the tag it was pulled by has already moved, `--pull=always` and a fresh host both take the new one, and nothing reports which of the two is running.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `podman` 5.x on 2026-08-18.
