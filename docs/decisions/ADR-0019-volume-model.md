# ADR-0019: Persistent volume model — default home volume plus named volumes

## Context and Problem Statement

The spec said only that build caches "may be mounted as persistent volumes." Because the guest root filesystem derives from the immutable store output, everything the guest writes is ephemeral unless placed on host-backed storage; teardown (ADR-0018) also needs a precise inventory of what data a project owns.

## Considered Options

- A single anonymous volume per project, nothing else
- Free-floating user-named volumes shareable across projects
- One default home volume plus project-scoped named volumes

## Decision Outcome

Chosen option: **default home volume plus project-scoped named volumes** — persistence-by-default where users expect it, with room for isolated extra disks.

- The guest filesystem is three layers: immutable image, ephemeral runtime layer, persistent volumes. The workspace remains a share (ADR-0009/0017), outside this model.
- Every volume is one host-side disk image attached as a block device, stored under the **state** root at `projects/<project-id>/<target>/volumes/<name>.img` — state, never cache, because volume contents are user data and not regenerable.
- The **default volume** always exists, reserved name `default`, mounted at the guest user's home.
- **Named volumes** are declared in the manifest (`[[volumes]]` with `name`, `mount`) or contributed by pieces through the module merge (lists concatenate); the piece-side option is named by ADR-0041. The same name declared with conflicting mountpoints fails evaluation; `viv volume list` names each volume's declaring layer.
- `[volume].persist` lists extra guest paths bind-mounted from inside the default volume.
- Identity is (project, target, name); manifests declare the shape, each bound project instantiates privately, and reattachment on `start` is automatic. No cross-project sharing.
- CLI is lifecycle-only: `viv volume list | rm <name> | rm --all`; creation is declarative. Removal refuses while the VM runs; absent targets are a no-op exit `0`.

## Consequences

- Good: caches, dotfiles, and user-run databases persist by default; destructive scope is always enumerable before `destroy`.
- Bad: guest writes outside `$HOME`, a volume mountpoint, or a `persist` path are silently ephemeral — the docs must state this loudly.

## Status

Accepted

Amended by **ADR-0037** — each volume image is sparse raw, created lazily on first `start`, and its declared size is a virtual ceiling reclaimed by trim rather than a preallocated amount. The model above is unchanged.

Amended by [`ADR-0041-resource-and-volume-channel-classification.md`](./ADR-0041-resource-and-volume-channel-classification.md) — the piece-side declaration surface is the `vivarium.volumes` option, and volume declarations are classified **build-channel**: the guest mountpoint is guest system configuration, so adding a volume rebuilds. The model above is unchanged.

Amended by [`ADR-0057-manifest-grammar-and-validation-boundary.md`](./ADR-0057-manifest-grammar-and-validation-boundary.md) — the volume shape gains an optional `size_gib`, the key that expresses the raisable virtual ceiling ADR-0037 introduced, and the conflicting-mountpoint check above is fixed at evaluation time (`65`) rather than at parse. The model above is unchanged.
