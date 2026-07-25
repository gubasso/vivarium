# ADR-0017: Workspace mount path and extra mounts

## Context and Problem Statement

ADR-0009 already decided launch-time injection and a fixed in-guest path, but the path value and
extra mount model were not named. Users need multiple sessions in the same project VM to see the same
workspace at a stable path while preserving N5.

## Considered Options

- Mirror the host path inside the guest.
- Use one fixed project-scoped path `/workspaces/<repo>` with launch-time host-path injection and
  optional additional runtime mounts.
- Use a single generic path such as `/workspace`.

## Decision Outcome

Chosen option: **one fixed project-scoped path `/workspaces/<repo>` with launch-time host-path
injection and optional additional runtime mounts**.

The primary workspace mount is `/workspaces/<repo>`. Additional mounts may be declared for other
guest paths, but their host sources are launch-time or personal/machine-local inputs and must not
enter shared build/config outputs. Multiple `exec`/`shell` sessions for a project share the same VM
and see the same mounts.

## Consequences

- Good: stable, project-scoped guest path without leaking host paths.
- Good: extra mounts cover multi-directory workflows without changing the primary workspace
  convention.
- Bad: mount declaration/source binding needs later schema detail and validation.

## Status

Accepted

Amended by
[`ADR-0020-mount-and-config-mirroring-schema.md`](ADR-0020-mount-and-config-mirroring-schema.md) —
the deferred mount declaration schema (the "Bad" consequence above) is now decided: declarative
`[[mounts]]` entries with launch-time per-side expansion. The path model decided here stands.

Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md)
and [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md).
