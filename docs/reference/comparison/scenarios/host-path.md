# Host path

Given that the project is visible inside — [the row above](./workspace-mount.md) — record whether the path it occupies inside is the path it occupies outside. Agents key session state on the working directory, so a mirrored tail is not the same answer as a mirrored path.

1. Put a project at a known absolute path on the host.
2. Give the tool that directory by its documented mechanism.
3. Inside, run `pwd` and read the path of a file the host also sees.
4. Record both paths, and whether a tool that stores absolute paths still resolves them.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: the [host-symmetric mount rule](../../spec/08-invariants-and-guarantees.md) fixes the workspace at the absolute path it occupies on the host, and a declared mount carries that same target rather than one the declaration invents.

## flake-pilot

flake-pilot `main`, read 2026-08-18. n/a: nothing crosses at the firecracker boundary, so there is no inside path to compare — the same reason the [mount-choice row](./choosing-mounts.md) and the [session-directory row](./session-sockets.md) read `n/a` here. The published `--volume %HOME/ai:%HOME/ai` that does mirror a path is the `crun` container backend, and it mirrors a quarantine directory rather than the project tree.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: since 1.0.0 a workspace under `$HOME` mounts at `/home/aiuser/<path relative to $HOME>` — a mirror of the tail, not of the path. The change answers the same failure class vivarium's [host-symmetric mount rule](../../spec/08-invariants-and-guarantees.md) does and stops short of it: an agent that stored `/home/you/api` reads a path that is not there, and a project outside `$HOME` has no tail to mirror.

## podman

podman 5.x, read 2026-08-19. Reachable, nothing arranges it: `-v /host/path:/host/path` mirrors any single path exactly, and under krun the mount crosses as virtiofs, so it holds at the compared setup. The user types the path twice at every invocation, nothing refuses a mismatch, and published examples usually pick a different target.
