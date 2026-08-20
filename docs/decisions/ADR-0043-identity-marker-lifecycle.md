# ADR-0043: The identity marker's lifecycle belongs to the VM-lifecycle verbs

## Context and Problem Statement

[`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md) said vivarium resolves and writes the `.vivarium/` marker "on every invocation" but never named a command. [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) excludes `viv init` from touching the project tree and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md) excludes all nine read-only diagnostics from mutating project state — between them, no writer was left. At the other end, spec/15 has `destroy` remove the marker while the teardown boundary in spec/10, spec/01, and ADR-0018 says teardown never touches the workspace, and no ADR recorded the removal at all.

## Considered Options

- `viv init` mints, amending its never-touches-the-tree rule.
- Every mutating verb mints on first touch.
- Only the shared ensure-running routine mints; `destroy` removes.

## Decision Outcome

Chosen option: the ensure-running routine mints, `destroy` removes — the marker exists to scope a VM's state, so it appears when a VM first does and leaves when that state is torn down.

- Minting verbs are `viv start` and `exec`/`shell` when they cold-start. Resolution still happens on every invocation; persistence does not, so a read-only command resolves in memory and writes nothing.
- `init --write` does not mint despite writing: the registry keys on the project's absolute path, not on `<project-id>` (ADR-0011), so recording a binding needs no identity.
- "Every mutating verb" was rejected as self-contradictory — `destroy` must remove the marker rather than create one, and `stop` on a never-started project is a no-op at exit `0`, where minting would surprise.
- `destroy` removes the marker and clears the identity-index entry, leaving the binding. The teardown boundary must carve this out: the marker is vivarium-owned, never user-authored (N9, N21).

## Consequences

- Good: one writer, and the read-only diagnostics guarantee becomes literally true.
- Good: spec/15's global index lock before the per-project `flock` already assumed concurrent first-time starts.
- Bad: an unpersisted id is deterministic but not stable — two copies both resolve to `api-2` until one starts.
- Bad: any acceptance check of the marker needs a bootable host, because the cheapest minting verb is `start`.

## Status

Superseded

Superseded by [`ADR-0107-the-sandbox-keys-on-the-manifest.md`](./ADR-0107-the-sandbox-keys-on-the-manifest.md) — 2026-08-20, because the marker whose lifecycle this record assigns no longer exists. With the sandbox keyed on the manifest, nothing is written into the project tree, so there is no file for a starting verb to create or for `viv destroy` to remove, and the teardown carve-out this record added to [`ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](./ADR-0018-lifecycle-verbs-and-teardown-boundary.md) lapses with it.

Amends [`ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](./ADR-0018-lifecycle-verbs-and-teardown-boundary.md) and [`ADR-0029-project-identity-and-marker.md`](./ADR-0029-project-identity-and-marker.md). Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md), [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md), and [`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md).
