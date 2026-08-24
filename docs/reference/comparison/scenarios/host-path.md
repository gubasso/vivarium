# Host path

Given that the project is visible inside — [the row above](./workspace-mount.md) — record whether the path it occupies inside is the path it occupies outside. Agents key session state on the working directory, so a mirrored tail is not the same answer as a mirrored path.[^read]

1. Put a project at a known absolute path on the host.
2. Give the tool that directory by its documented mechanism.
3. Inside, run `pwd` and read the path of a file the host also sees.
4. Record both paths, and whether a tool that stores absolute paths still resolves them.

## vivarium

Yes: the [host-symmetric mount rule](../../spec/08-invariants-and-guarantees.md) fixes the workspace at the absolute path it occupies on the host, and a declared mount carries that same target rather than one the declaration invents. Since `162f230` that holds for a set rather than one tree — every declared workspace is mirrored at its own host path, and a pair where one contains the other is refused before boot, because two nesting trees cannot both be mirrored at their own paths ([ADR-0108](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md)).

## flake-pilot

At the firecracker route, n/a: nothing crosses, so there is no inside path to compare — the same reason [the mount-choice row](./choosing-mounts.md) and [the session-directory row](./session-sockets.md) read `n/a` for it.

At the `krun` route, reachable and nothing checks it: the upstream registration's `--opt "\--volume %HOME/ai:%HOME/ai"` names the same path on both sides, and under `krun` the mount crosses as virtio-fs, so a file's path inside is the path it had outside. Two things keep it an arrangement rather than an answer. The mirroring is a property of how the person who wrote the registration happened to write it — `%HOME/ai:/work` would have been accepted identically, and nothing reports a mismatch — and what mirrors is `~/ai`, a quarantine directory the upstream flow tells you to create, rather than the project tree wherever it already lives.

## glaipnir

No: since 1.0.0 a workspace under `$HOME` mounts at `/home/aiuser/<path relative to $HOME>` — a mirror of the tail, not of the path. The change answers the same failure class vivarium's [host-symmetric mount rule](../../spec/08-invariants-and-guarantees.md) does and stops short of it: an agent that stored `/home/you/api` reads a path that is not there, and a project outside `$HOME` has no tail to mirror.

## podman

Reachable, nothing arranges it: `-v /host/path:/host/path` mirrors any single path exactly, and under krun the mount crosses as virtiofs, so it holds at the compared setup. The user types the path twice at every invocation, nothing refuses a mismatch, and published examples usually pick a different target.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19. `vivarium` re-read at `162f230` on 2026-08-22, where mirroring generalized from one tree to a declared set.
