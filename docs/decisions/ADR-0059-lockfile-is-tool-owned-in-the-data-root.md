# ADR-0059: The lockfile is tool-owned, per target, in the data root

## Context and Problem Statement

N3 and [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md) rest determinism on "a pinned lockfile" without naming one: which file, who creates it, when it moves, and whether it is shared across projects were all open. vivarium cannot commit a lock into the project (N9) and cannot write the config root (N13), so the usual answer — a lock beside the manifest — is unavailable.

## Considered Options

- One global lockfile per user.
- One lockfile per project target, tool-owned, under the data root.
- A lockfile committed beside the manifest in the config root.

## Decision Outcome

Chosen option: **one lockfile per project target, under the data root** at `projects/<project-id>/<target>/flake.lock`.

- **Data**, because a lockfile is a pin and the data root is exactly "pinned inputs" (`spec/02`). Cache is documented deletable, and deleting a lock does not rebuild — it re-resolves, which is N3's failure mode. State is cleared by `viv destroy`, which would silently discard the pin.
- **Per target, not global.** A global lock makes updating one project an unannounced update to every other. ADR-0049 already speaks of "the project's lockfile".
- **Created on first build and announced, never demanded.** Failing closed governs manifest _resolution_ (N7), not a derived pin the tool owns; refusing a user's first `start` would make ADR-0004's "no Nix fluency required" false. The safeguard is printing what was pinned.
- **Only `viv update` moves it.** No ordinary build re-resolves inputs.
- **A team's shared pin is a read-only `flake.lock` in the config root**, which wins when present and is never written — the config root is where ADR-0040 already put shared guarantees.
- **Each generation retains the lock that built it**, not merely a revision: a revision alone cannot reproduce an evaluation.

## Consequences

- Good: determinism finally names a file, and rollback is genuine — a generation carries its own pin.
- Good: two projects can move independently.
- Bad: without the config-root override lock, two teammates can build one manifest against different pins.
- Bad: a new verb and a new per-target file, plus a per-generation lock snapshot to retain.

## Status

Accepted

Amends [`ADR-0014-build-generations-and-gc-roots.md`](./ADR-0014-build-generations-and-gc-roots.md) — the per-generation record retains a snapshot of the lockfile and its digest, replacing the bare lock revision, because the revision alone does not reproduce an evaluation.

Amends [`ADR-0004-toml-manifest-compiles-to-flake.md`](./ADR-0004-toml-manifest-compiles-to-flake.md) — the generated flake reads a lockfile it does not own, staged in from the path above.

Amends [`ADR-0049-backend-is-a-closure-member.md`](./ADR-0049-backend-is-a-closure-member.md) — the backend is still pinned by the project's lockfile, but the generation record beside which it is written is the retained lock snapshot and its digest, not a bare revision.

Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md), [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md), and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
