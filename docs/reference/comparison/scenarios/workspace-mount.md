# Workspace mount

Two questions hide in "can I see my project inside". This one asks whether the tool puts the project there without being asked; [the next](./host-path.md) asks whether the path it lands on is the one it had outside.

1. Put a project at a known absolute path on the host.
2. Start the tool for that project with no mount argument.
3. Record whether the project is visible inside, and what named it.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the workspace is the project the manifest belongs to, so binding the project once is what names it, and no argument repeats the decision at each start. Project identity is anchored by a marker rather than by the path, so the mount survives a rename.

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker schema has no bind mount, so nothing is arranged and nothing can be. `include.tar` and `include.path` copy a payload into the artifact at provisioning time — work is copied, not seen through.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the invocation's workspace crosses with no argument naming it, and a workspace equal to `$HOME` is refused and falls back rather than crossing wholesale. Where it lands is [the next row](./host-path.md#glaipnir), and it is not where it came from.

## podman

podman 5.x, read 2026-08-19. No: `-v` is the only route and it is typed at the call site every time. Nothing reads the current directory, and a run that omits the flag starts a container that cannot see the project at all.
