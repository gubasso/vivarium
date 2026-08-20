# ADR-0107: The sandbox keys on the manifest

## Context and Problem Statement

A sandbox is keyed by `<project-id>`, the sanitized project-directory name anchored in a `.vivarium/` marker ([`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md)), and a project resolves to one manifest through the state registry (N7). One directory therefore gets one VM, so a family of related repositories cannot share a sandbox however alike their environments are. The key also costs a derivation nothing else needs: sanitization, a length cap, collision suffixes, a mint lock, and a move-versus-copy resolution table.

## Considered Options

- Keep the project-directory key.
- Key on the manifest name.
- Key on a hash of the declared workspace set.

## Decision Outcome

Chosen option: `Key on the manifest name` — a manifest already describes exactly one image, and manifest names are unique by construction in one library, so identity needs no derivation at all.

`<sandbox-id>` is the manifest name; every per-sandbox state and runtime path keys on it, with the `<target>` component unchanged. The registry inverts: it stops being the written home of a project-to-manifest binding and becomes a derived index from declared workspaces back to the manifest that owns them, rebuildable from the manifests alone. The `.vivarium/` marker, the identity index, suffix minting, and the move-versus-copy resolution are deleted.

The hash was rejected because it names the sandbox after a set the user may edit, so an added workspace would silently strand the previous sandbox's state.

## Consequences

- Good: the projects one manifest declares share one VM by construction, which is the capability this key exists to give.
- Good: N9 loses its sole exception and becomes absolute — nothing is written into a project's own tree.
- Bad: the inversion holds only while manifests stay personal ([`./ADR-0040-manifest-is-the-personal-layer.md`](./ADR-0040-manifest-is-the-personal-layer.md)); distributing manifests reopens it.
- Bad: state under the old key is not migrated. `viv destroy` then `viv start` is the path, which is what disposability is for.

## Status

Proposed

Supersedes [`./ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md), [`./ADR-0029-project-identity-and-marker.md`](./ADR-0029-project-identity-and-marker.md), [`./ADR-0043-identity-marker-lifecycle.md`](./ADR-0043-identity-marker-lifecycle.md), and [`./ADR-0054-stale-bindings-surfaced-not-reaped.md`](./ADR-0054-stale-bindings-surfaced-not-reaped.md).

Amends N7, N9, and N21 in [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md). Enacted by [slice 021](../plan/slices/021-the-manifest-is-the-sandbox/README.md).
