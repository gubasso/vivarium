# ADR-0054: Stale bindings are surfaced, never reaped

## Context and Problem Statement

A registry entry can point at a directory that no longer exists, and nothing said who removes it. Removing it automatically is not merely cautious to avoid — it is unsafe. [`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md) tells a move from a copy by whether a recorded path still exists, so a reaper destroys exactly the evidence that makes a move a move, and a moved project returns as a first-time mint with a fresh suffix and orphaned state. A vanished path may also be nothing worse than an unmounted filesystem.

## Considered Options

- Prune stale entries lazily, on read or on `viv status -g`.
- Keep the removal on the command that reports it: `viv status -g --clean`.
- Warn on report, and give the removal its own verb.

## Decision Outcome

Chosen option: warn, and give removal its own verb — `viv status` is declared pure read-only in three places, and a mutating mode on the one command that runs unattended in scripts is worth more than the verb it saves.

- `viv status -g` warns on stderr, naming each stale entry, saying why nothing was removed automatically, and naming the command that would remove it. Under `--json` the warning is the `path_missing` field and no prose is emitted.
- `viv unbind [<path>] [--stale] [--yes]` removes registry entries, rechecking absence under the exclusive registry lock immediately before deleting — a filesystem remounted in between is a no-op, not a data loss.
- Identity-index entries are never removed this way; only `viv destroy` removes those.

## Consequences

- Good: `viv init --write` gains the inverse the surface was missing, and the read-only guarantee survives untouched.
- Good: the unmount case degrades to a warning a user can ignore.
- Bad: one more verb on a command surface that was considered closed.
- Bad: cleaning is two commands — see the problem, then run the fix.

## Status

Accepted

Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md). Completes the explicit-and-reversible principle of [`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md).
