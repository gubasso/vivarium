# ADR-0108: A workspace is owned by one manifest

## Context and Problem Statement

[`./ADR-0100-the-workspace-mirrors-its-host-path.md`](./ADR-0100-the-workspace-mirrors-its-host-path.md) mirrors one primary workspace at its host path while [`./ADR-0020-mount-and-config-mirroring-schema.md`](./ADR-0020-mount-and-config-mirroring-schema.md) lands every other declaration at a chosen `target`. Once a sandbox keys on the manifest ([`./ADR-0107-the-sandbox-keys-on-the-manifest.md`](./ADR-0107-the-sandbox-keys-on-the-manifest.md)), a manifest declares several project trees and the two kinds must be told apart. A directory may also be declared by more than one manifest, so a working directory no longer names one sandbox.

## Considered Options

- A `kind = "workspace"` discriminator on `[[mounts]]`.
- A separate `[[workspaces]]` table, with ownership unique per directory.
- An omitted `target` meaning mirror.

## Decision Outcome

Chosen option: `A separate [[workspaces]] table` — a workspace mirrors its host path and therefore has no `target` to declare, so it is not a `[[mounts]]` row with a flag on it. Absence of a field is a weak signal; a table is an explicit one.

The two verbs differ in what they claim. `[[mounts]]` says a directory is visible in this VM, and any number of manifests may say it about one directory. `[[workspaces]]` says a directory belongs to this VM, and at most one manifest may claim a given directory. Ownership being single-valued is what keeps resolution from a working directory unique, the way a directory lies in exactly one git repository.

Every declared workspace is mirrored at its own host path, generalizing N16 beyond the primary tree, and ADR-0100's refusal set — equal to, containing, or under a path the guest owns — runs pairwise across the set.

## Consequences

- Good: overlap stays legal as mounts, so nothing a user wants to share is lost, and no per-invocation disambiguation is needed.
- Good: the workspace-to-manifest index is derivable from the manifests alone, so it can be deleted and rebuilt.
- Bad: two tables where there was one, and a piece must declare the kind it means.
- Bad: no entry is privileged, so the session's starting directory must derive from the invoking directory rather than from a first-declared tree.

## Status

Implemented

Amended by [`ADR-0110-the-workspace-is-an-ordinary-mount.md`](./ADR-0110-the-workspace-is-an-ordinary-mount.md) — on the compiled form, not the authored one. The `[[workspaces]]` table and its single-valued ownership stand exactly as decided here. What changes is that the table compiles to a mount row whose `target` equals its `source` rather than to a share family of its own, so "not a `[[mounts]]` row with a flag on it" describes the surface a user writes rather than the merged configuration.

Amends N16 in [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md), which scopes host-symmetric mirroring to the primary workspace.

Amends [`ADR-0020-mount-and-config-mirroring-schema.md`](./ADR-0020-mount-and-config-mirroring-schema.md), whose schema keeps the `target`-mounted kind and loses the project tree to a table of its own, and [`ADR-0100-the-workspace-mirrors-its-host-path.md`](./ADR-0100-the-workspace-mirrors-its-host-path.md), whose mirroring and refusal set now run across a set rather than one tree. Both stand and carry the pointer back.

Enacted by [slice 020](../plan/slices/020-many-workspaces-in-one-sandbox/README.md): the explicit table, complete launch set, pairwise refusal, and exact session-directory mapping are implemented and exercised by `workflow_20_many_workspaces_one_sandbox`.
