# ADR-0090: The guest store volume is listed

## Context and Problem Statement

[`ADR-0087`](./ADR-0087-the-inner-store-persists-on-its-own-volume.md) gives the guest store a volume nobody declared, and [`ADR-0089`](./ADR-0089-the-guest-store-is-collected-on-space-pressure.md) bounds its growth without capping it. `viv volume list` reports declared volumes and the default home, so the largest thing a project silently accumulates would be the one volume a user cannot see the size of — and `viv volume rm` already offers to reclaim it, which is an operation offered on a row that is never printed.

## Considered Options

- **Omit it** — it is vivarium's bookkeeping, not the user's data.
- **List it behind a flag**, `--all`.
- **List it unconditionally**, like every other volume.

## Decision Outcome

Chosen option: **list it unconditionally.**

- **The schema already fits.** `declared_by: null`, exactly as the default home volume — the other volume that exists without declaration — already uses. No new field, no new command, no second notion of "volume".
- **A hidden cost is still a cost.** It carries the same 32 GiB ceiling as any other and holds everything the inner layer fetched for itself ([`ADR-0084`](./ADR-0084-the-inner-layer-provisions-its-own-store.md)). A full one is how a project gets stuck, and `allocated_bytes` against `virtual_bytes` is the only warning before that.
- **A flag is refused.** `--all` would make the default view a partial one, so "how much disk is this project using?" would have two answers, one of them wrong.
- **It is not an orphan and never a prune candidate**, even though it matches the orphan predicate's letter — declared by no current layer — because it is entirely live. That exclusion is stated where the predicate is.

## Consequences

- Good: total project disk is one `viv volume list` away, and `rm` becomes discoverable rather than documented.
- Bad: the table now mixes rows that answer to the manifest with one that does not — its size is not settable there.
- Bad: any test asserting an exact row set must count it.

## Status

Accepted

Companion to [`ADR-0089`](./ADR-0089-the-guest-store-is-collected-on-space-pressure.md), split from it on review: the two answer one discovery but reverse independently. Specified in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), with the row in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md).

**Unimplemented**, like the command it extends — `viv volume` is spec plus a gated acceptance test ([`../reference/implementation-status.md`](../reference/implementation-status.md)), so this costs a row in that test rather than a change to working output.

**Listing an undeclared volume is the ordinary practice.** Docker and Podman both list anonymous volumes; Kubernetes' failure to account for implicitly created ones is a standing complaint. The argument in the negative is the stronger one, which is why the row is not optional.
