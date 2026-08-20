# ADR-0029: Project identity is the directory name, anchored by a gitignored marker

## Context and Problem Statement

Every per-project directory on disk — build generations, volumes, the runtime lock, and the control socket — is keyed by a `<project-id>` the specs never defined (spec/11 and spec/12 left it "still open"). That key must be stable when a project directory is moved or renamed, auto-disambiguate collisions, and stay short enough for a Unix-socket path — all without a manual step.

## Considered Options

- Canonical-path hash — derive the id by hashing the project's real path.
- Registry-only name — record the basename per path in the state registry; reconcile moves heuristically when the path no longer matches.
- Name + gitignored marker — the basename is the id, persisted in a vivarium-owned marker inside the project so identity travels with the directory.

## Decision Outcome

Chosen option: name + gitignored marker. The id is the sanitized directory basename, suffixed (`-2`, `-3`, …) only on a live collision. It is persisted in a self-ignoring `.vivarium/` marker (`.vivarium/id`, beside a `.gitignore` of `*`), so a move or rename is a no-op instead of orphaning state the way a path hash would. The per-user state registry records `id → live canonical path`, which separates a move (old path gone → keep the id) from a copy or second checkout (old path still live → mint a fresh suffix). The full resolution algorithm and corner cases are owned by [`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md).

The marker carries identity only. The manifest binding still lives in the state registry ([`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md)); the resolution precedence (N7) is unchanged.

## Consequences

- Good: human-readable, socket-safe ids; moves and renames survive automatically; copies and clones self-disambiguate.
- Good: the inner dev environment is untouched — the marker is inert and self-ignored.
- Bad: vivarium now writes one directory into the project tree, narrowing N9; after a leaf-rename the id stays the old name, not the new one.

## Status

Superseded

Superseded by [`ADR-0107-the-sandbox-keys-on-the-manifest.md`](./ADR-0107-the-sandbox-keys-on-the-manifest.md) — 2026-08-20, by removing the problem rather than solving it differently. A sandbox keys on the manifest name, which is already unique by construction in one library, so nothing has to be derived from a directory: the sanitization, the length cap, the collision suffix, the marker, the identity index, the global mint lock, and the move-versus-copy-versus-clone resolution table are deleted rather than replaced. The N9 exception this record won is given back, and N9 becomes absolute.

Amended by [`ADR-0043-identity-marker-lifecycle.md`](./ADR-0043-identity-marker-lifecycle.md) — the marker's lifecycle is assigned: `viv start` and a cold-starting `exec`/`shell` write it, `viv destroy` removes it, and every other command resolves the identity without persisting it. The derivation and resolution algorithm above are unchanged.

Amends N9 ([`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md)) — vivarium may manage a self-ignored `.vivarium/` runtime marker in the project tree (identity only). Amends [`ADR-0008-two-layer-separation.md`](./ADR-0008-two-layer-separation.md) and [`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md). Specified in [`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md).
