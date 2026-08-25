# Choosing mounts

Every subject decides what crosses. This row asks where that decision is written down: in a file, or in the arguments of the command that starts it. Whether that file travels with the project is [Defined by a project file](./project-file.md), and whether a second author can add a path without editing the first is [Compose the environment from separate, reusable parts](./composition.md); neither is re-asked here.[^read]

1. Add a host path to what crosses, and a second one that must not.
2. Record where that decision is recorded.
3. Record what a run that names neither path then sees.

## vivarium

Yes: `[[mounts]]` is a table in the manifest, so the set is part of the project's definition rather than of an invocation, and no command adds a path the manifest does not show. Because layers merge as NixOS modules and lists concatenate, a piece contributes mounts to the same list without editing the manifest that imported it, and a shared layer may only reach the host through portable variables — `${HOME}` and the four durable XDG directories — with a literal personal path failing evaluation with `65`. Two floors bound the choice rather than the user: a mount whose source resolves to a session directory is refused before boot, and what does cross carries the [host-symmetric](../../spec/08-invariants-and-guarantees.md) target rather than one the declaration invents. Since `162f230` the project tree is chosen the same way, in a second table: `[[mounts]]` names paths this sandbox borrows, `[[workspaces]]` names trees it owns, and neither has a command-line form ([ADR-0108](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md)).

What crosses is two tables in the project's own file, so reading the manifest is reading the crossing set:

```toml
[[workspaces]]
source = "${HOME}/projects/api"       # a tree this sandbox owns, mirrored at its host path

[[mounts]]
source   = "${HOME}/.config/gcloud"   # portable variable, not a literal personal path
target   = "~/.config/gcloud"
readonly = true
```

## flake-pilot

At the firecracker route, n/a: there is no bind-mount mechanism, so there is no set to choose from — the same reason [the session-directory row](./session-sockets.md) reads `n/a` for it. What the registration can carry is `include.tar` / `include.path`, which copies material onto the instance at provisioning time rather than selecting what crosses at run time.

At the `krun` route, reachable and recorded in a file: what crosses is a list of `--opt "\--volume ..."` lines, and `flake-ctl podman register` writes them into `/usr/share/flakes/<app>.yaml`, where a second concern can add to them through an `<app>.d/` drop-in without editing the first. So the decision is written down rather than typed at each start, which is what this row asks. What the file is not is a file that travels with the project — it lives in a system directory and is keyed by the registered command name, which is [Defined by a project file](./project-file.md#flake-pilot). And nothing arranges the list: a path nobody names does not cross, and nothing notices that the one you meant is missing.

## glaipnir

No: the crossing set is written in the script. `_bind_agent_mounts` emits a fixed `--volume` list per agent name, and the workspace, hooks, and cache mounts are assembled at the call site beside it. There is no configuration key that adds a path, so a user who wants one edits `glaipnir.sh` — which is the same built-in opinion that wins glaipnir the [credential-scoping row](./per-tool-credentials.md) and costs it every tool outside the roster.

## bunkerbox

No: the mount set is fixed. What crosses is the project tree, the tool's own persisted home, a generated resolver file, and a generated wrapper directory — and none of the four config surfaces has a key for adding a fifth. The nearest thing to one is a sandbox profile's `paths`, and it faces the other way: those are host directories a passthrough command may see while it runs on the host, not host directories the guest may see.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18, re-read at `162f230` on 2026-08-22 for the workspace table; `flake-pilot` `44e3ab2` on 2026-08-20; `glaipnir` `8c7420e`, read 2026-08-20 — a later revision than the `21ef389` the rest of this subject is pinned to, read fresh for this row; `bunkerbox` `b7f14f3` on 2026-08-25.
