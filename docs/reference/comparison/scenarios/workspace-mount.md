# Workspace mount

Three questions hide in "can I see my project inside". This one asks the narrowest of them: whether the tool works out which project it is looking at, with no host path named anywhere. [The next](./host-path.md) asks whether the path it lands on is the one it had outside, and [the mount-choice row](./choosing-mounts.md) asks whether the decision, however it was made, was recorded in a file or retyped at each start. A tool can pass that last one and fail this one, and one of the subjects here does.[^read]

1. Register or configure the tool as its own documentation intends, and stop before starting it.
2. Put a project at a known absolute path on the host.
3. Start the tool for that project with no mount argument.
4. Record whether the project is visible inside, what named it, and whether anything in what was named is a host path a person wrote.

## vivarium

Yes: the workspace is the project the manifest belongs to, so binding the project once is what names it, and no argument repeats the decision at each start. Project identity is anchored by a marker rather than by the path, so the mount survives a rename.

## flake-pilot

No at both routes, for opposite reasons — which is the distinction this row exists to draw.

At the firecracker route there is no mount at all: the schema has no bind-mount key, and `include.tar` and `include.path` copy a payload onto the instance at provisioning time, so work is copied rather than seen through. Nothing derives the workspace because nothing carries one.

At the `krun` route the mount is real, arranged by the tool, and still not derived. The upstream registration freezes `--opt "\--volume %HOME/ai:%HOME/ai"` and `--opt "\--workdir %HOME/ai"` into `/usr/share/flakes/<app>.yaml`, and `podman-pilot` replays both on every `podman create`, so step 2 finds the directory there with no argument typed. What named it is a host path a person wrote at registration, and the registration is per application rather than per project: a registered `claude` mounts whatever was named when it was registered and does not follow you into a different project tree. Two projects wanting two workspaces are two registered command names.

That the decision was recorded once rather than retyped is a real property and it is credited, on [the row that asks where the decision is written down](./choosing-mounts.md#flake-pilot). This row asks the other half — whether the tool needed to be told at all.

## glaipnir

Yes: the invocation's workspace crosses with no argument naming it, and a workspace equal to `$HOME` is refused and falls back rather than crossing wholesale. Where it lands is [the next row](./host-path.md#glaipnir), and it is not where it came from.

## podman

No: `-v` is the only route and it is typed at the call site every time. Nothing reads the current directory, and a run that omits the flag starts a container that cannot see the project at all.

Both halves are typed at every start, and a run that omits them starts a guest with no project in it:

```bash
cd ~/projects/my-thing
podman run --rm -it --runtime krun -v "$PWD:$PWD" -w "$PWD" docker.io/library/node:22 bash
```

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19. The `flake-pilot` `krun` route and the protocol's registration step were added at `920f41e` on 2026-08-21.
