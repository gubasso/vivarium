# ADR-0100: The workspace mirrors its host path

## Context and Problem Statement

[`ADR-0017-workspace-mount-path-and-extra-mounts.md`](./ADR-0017-workspace-mount-path-and-extra-mounts.md) fixed the primary workspace at `/workspaces/<repo>`, a path existing only inside the guest. Git's linked worktrees record two absolute paths: a forward pointer under the main repository's `.git/worktrees/`, and a back-pointer in the worktree's own `.git` that `git worktree add` writes from the repository it resolved through the current directory, which no argument overrides. A tree reachable only at a guest-invented path therefore yields worktree metadata valid on one side.

The repair is blocked: a share's mount point is build output, which N19 keeps host paths out of.

## Considered Options

- Keep the fixed project-scoped path `/workspaces/<repo>`.
- Mirror the host path as the share's own `mountPoint`.
- Mirror the host path with a launch-time bind above a build-time constant.

## Decision Outcome

Chosen option: `launch-time bind above a build-time constant` — the only option reaching path symmetry without putting a host path in a build output.

The share mounts at `/run/vivarium-workspace`. The launcher percent-encodes the host path onto the kernel command line as `vivarium.workspace=`, and a guest unit binds the share there before the agent accepts a session. The session working directory is the mirrored path.

Not under `/run/vivarium`: the agent's `RuntimeDirectory=vivarium` has systemd delete that directory whenever the unit restarts.

A mirrored path equal to, containing, or under a path the guest owns is refused at exit `78`.

## Consequences

- Good: git records one absolute path string that resolves on both sides.
- Good: N19 and [`ADR-0009-launch-time-workspace-path-injection.md`](./ADR-0009-launch-time-workspace-path-injection.md) hold, so the guest closure stays byte-identical across hosts and no host path reaches the store.
- Bad: `/proc/cmdline` is world-readable in the guest, so the host user's name and directory layout reach every guest process. The mount path discloses them anyway.
- Bad: the guest home is `/home/vivarium`, so a host user of that name cannot be mirrored.
- Bad: the tree is reachable at two guest paths — the ambiguity this record otherwise removes.
- Bad: not sufficient for a linked worktree, whose `.git` points outside the shared tree. The main repository must be shared too, through the declared-mount surface, not yet enacted.

## Status

Proposed

Supersedes [`ADR-0017-workspace-mount-path-and-extra-mounts.md`](./ADR-0017-workspace-mount-path-and-extra-mounts.md), whose extra-mount model stands and whose primary-path model this replaces.

Amends N16 in [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md), which forbade a host-derived mount path. Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md).
