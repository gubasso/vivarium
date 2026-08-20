# Workspace mount

Two questions hide in "can I see my project inside". This one asks whether the tool puts the project there without being asked; [the next](./host-path.md) asks whether the path it lands on is the one it had outside.[^read]

1. Put a project at a known absolute path on the host.
2. Start the tool for that project with no mount argument.
3. Record whether the project is visible inside, and what named it.

## vivarium

Yes: the workspace is the project the manifest belongs to, so binding the project once is what names it, and no argument repeats the decision at each start. Project identity is anchored by a marker rather than by the path, so the mount survives a rename.

## flake-pilot

No: the firecracker schema has no bind mount, so nothing is arranged and nothing can be. `include.tar` and `include.path` copy a payload into the artifact at provisioning time — work is copied, not seen through.

## glaipnir

Yes: the invocation's workspace crosses with no argument naming it, and a workspace equal to `$HOME` is refused and falls back rather than crossing wholesale. Where it lands is [the next row](./host-path.md#glaipnir), and it is not where it came from.

## podman

No: `-v` is the only route and it is typed at the call site every time. Nothing reads the current directory, and a run that omits the flag starts a container that cannot see the project at all.

Both halves are typed at every start, and a run that omits them starts a guest with no project in it:

```bash
cd ~/projects/my-thing
podman run --rm -it --runtime krun -v "$PWD:$PWD" -w "$PWD" docker.io/library/node:22 bash
```

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
