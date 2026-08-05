# ADR-0014: Build generations via a per-project Nix profile and GC roots

## Context and Problem Statement

`viv up --no-rebuild` boots the last build, and users want to list past builds and boot a specific one, like `home-manager generations`. But an unreferenced Nix store output is removed by `nix-collect-garbage`, so "the last build" is not durable unless something pins it. We need a retrievable, garbage-collection-safe history of a project's built sandboxes.

## Considered Options

- Bare `result` / `--out-link` symlinks in the project directory — one link per build.
- Per-project Nix profile under the state root — numbered generations, each symlink a GC root.
- A JSON index of store paths the tool manages itself, without GC roots.

## Decision Outcome

Chosen option: per-project Nix profile under the state root.

Each successful `up` build appends a numbered generation to the project's profile. Every generation symlink is a garbage-collector root, so retained builds survive `nix-collect-garbage` with no extra bookkeeping (N14). The profile gives listing and rollback semantics directly: `viv up --no-rebuild` boots the current generation, `viv up --generation <n>` a specific one, `viv generations` lists them, and `viv gc` prunes under a retention policy. Generations live under `state/vivarium/projects/<project-id>/<target>/`, never in the project tree (N9).

## Consequences

- Good: history is GC-safe by construction; no ad-hoc pinning to maintain.
- Good: rollback and "boot generation N" come from the profile, not custom code.
- Bad: each retained generation pins a full closure, so disk grows until `viv gc` prunes.
- Bad: generation numbers are monotonic and non-reused — deleting one leaves a gap.

## Status

Accepted

Amended by [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — a generation retains a snapshot of the lockfile that built it plus its digest, replacing the bare lock revision in the per-generation record, because a revision alone does not reproduce an evaluation. The profile-and-GC-root mechanism is unchanged.

Specified in [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md).
