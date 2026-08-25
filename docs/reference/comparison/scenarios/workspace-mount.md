# Workspace mount

Three questions hide in "can I see my project inside". This one asks the narrowest of them: whether the tool works out which project it is looking at, with no host path named anywhere. [The next](./host-path.md) asks whether the path it lands on is the one it had outside, and [the mount-choice row](./choosing-mounts.md) asks whether the decision, however it was made, was recorded in a file or retyped at each start. A tool can pass that last one and fail this one, and two of the subjects here do, vivarium among them.[^read]

1. Register or configure the tool as its own documentation intends, and stop before starting it.
2. Put a project at a known absolute path on the host.
3. Start the tool for that project with no mount argument.
4. Record whether the project is visible inside, what named it, and whether anything in what was named is a host path a person wrote.

## vivarium

No, and by a recorded decision rather than a gap: a workspace is declared, not discovered. Each `[[workspaces]]` row in the manifest names a host `source`, and a manifest that declares none cannot launch, so what puts the project inside is a path a person wrote. A host-side `${VAR}` may stand in for the literal, which keeps a personal path out of anything shared and does not change who named the tree.

Deriving the workspace from the invoking directory is what vivarium did until `162f230`, and it stopped working the moment a sandbox could hold more than one project tree: with a set, the invoking directory has to be looked up among the declared trees rather than be the answer, and a directory has to belong to exactly one sandbox for that lookup to be unique ([ADR-0108](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md)). Deriving it would also make the mount set a function of the call, so one configuration would name a different sandbox from every directory it was started in. What replaces the derivation is a refusal rather than a default: a command run from a tree the resolved manifest does not declare is specified to exit `78` naming the block to add, instead of starting quietly somewhere else ([ADR-0109](../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md), recorded and not yet built).

That the decision is written once in the project's own file rather than retyped at each start is a real property, and it is credited on [the row that asks where the decision is written down](./choosing-mounts.md#vivarium). This row asks the other half, whether the tool needed to be told at all, and vivarium now needs to be told — the same answer as flake-pilot's `krun` route, reached from the opposite direction: there because a registration is per command name, here because a sandbox holds a declared set of trees.

## flake-pilot

No at both routes, for opposite reasons — which is the distinction this row exists to draw.

At the firecracker route there is no mount at all: the schema has no bind-mount key, and `include.tar` and `include.path` copy a payload onto the instance at provisioning time, so work is copied rather than seen through. Nothing derives the workspace because nothing carries one.

At the `krun` route the mount is real, arranged by the tool, and still not derived. The upstream registration freezes `--opt "\--volume %HOME/ai:%HOME/ai"` and `--opt "\--workdir %HOME/ai"` into `/usr/share/flakes/<app>.yaml`, and `podman-pilot` replays both on every `podman create`, so step 2 finds the directory there with no argument typed. What named it is a host path a person wrote at registration, and the registration is per application rather than per project: a registered `claude` mounts whatever was named when it was registered and does not follow you into a different project tree. Two projects wanting two workspaces are two registered command names.

That the decision was recorded once rather than retyped is a real property and it is credited, on [the row that asks where the decision is written down](./choosing-mounts.md#flake-pilot). This row asks the other half — whether the tool needed to be told at all.

## glaipnir

Yes: the invocation's workspace crosses with no argument naming it, and a workspace equal to `$HOME` is refused and falls back rather than crossing wholesale. Where it lands is [the next row](./host-path.md#glaipnir), and it is not where it came from.

## bunkerbox

Yes: the tool resolves the repository root from the working directory and mounts it, and no configuration file or argument anywhere carries a host path. Typing the packaged command inside a project is the whole of it. Where it lands is [the next row](./host-path.md#bunkerbox), and it is not where it came from.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25. The `flake-pilot` `krun` route and the protocol's registration step were added at `920f41e` on 2026-08-21. `vivarium` re-read at `162f230` on 2026-08-22, where the derived workspace became a declared one.
