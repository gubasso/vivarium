# ADR-0018: Lifecycle verbs — start, stop, destroy, and the teardown boundary

## Context and Problem Statement

The surface used `viv up` / `viv down`, with `down` defined as a non-destructive stop and no teardown verb at all. A cross-tool survey showed local-VM tools converge on `start`/`stop` plus a separate `destroy`/`delete`, while one prominent container-composition ecosystem uses `down` destructively — making `down` semantically ambiguous across ecosystems and leaving vivarium without an "erase this project" verb.

## Considered Options

- Keep `up`/`down`, add a separate `destroy`
- `start`/`stop` plus a destructive `down`
- `start`/`stop`/`destroy`, no `down` verb at all

## Decision Outcome

Chosen option: **`start`/`stop`/`destroy`, no `down`** — matches the local-VM convention and removes the ambiguous verb entirely.

- **`start`** renames `up` unchanged: ensure built + running, idempotent, non-destructive (N15).
- **`stop [--force] [-t|--timeout <secs>]`** — graceful stop via the in-guest agent, falling back to backend soft-off, then hard poweroff at timeout (default 10 s; `-1` waits). `--force` powers off immediately and conflicts with a nonzero `--timeout` (usage error). Removes nothing; idempotent.
- **`destroy [-f|--yes] [--keep-volumes]`** — stop, unlink all generation GC roots, remove all persistent volumes and runtime state. Prompts on a TTY; non-interactive requires `--yes`. Never touches workspace, config, binding, or store contents; store paths are reclaimed only by a later collection. Idempotent.
- **`gc`** becomes a thin global store-collector wrapper; retention pruning moves to `viv generations prune`, and all generation management lives under `viv generations`.

## Consequences

- Good: unambiguous destructive verb; the stop/destroy boundary is expressible as an invariant (N18); grammar matches user expectations from comparable VM tools.
- Bad: renames verbs that ADR-0013/0015 prose records under their old names (both now carry "Amended by" pointers here; spec pages carry the new names), and `viv gc`'s meaning changes from the earlier design.

## Status

Accepted

Amended by [`ADR-0043-identity-marker-lifecycle.md`](./ADR-0043-identity-marker-lifecycle.md) — the teardown boundary above gains one carve-out: `destroy` also removes the vivarium-owned `.vivarium/` identity marker and clears the identity-index entry. Workspace, config, binding, and store contents are untouched as stated; the marker is vivarium's own file, not user-authored (N9, N21).
