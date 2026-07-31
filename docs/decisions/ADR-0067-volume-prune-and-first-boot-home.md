# ADR-0067: `volume prune` reclaims orphans only, and the guest owns its home on first boot

## Context and Problem Statement

`spec/06` defines an **orphan** — a volume image on disk that no current layer declares — and `volume list` reports the flag per row, but nothing acts on it in bulk. Separately, a volume is a fresh empty filesystem on its first `viv start`, so the default volume's root is unowned by the guest user who lives there.

## Considered Options

- **Leave both alone** — `volume rm` in a loop, and a recursive ownership fix at every boot.
- **A general `prune` with an `--all` that also removes declared volumes.**
- **An orphan-only `prune`, plus declarative first-boot ownership.**

## Decision Outcome

Chosen option: **orphan-only `prune` plus declarative first-boot ownership** — a verb without a new concept, and ownership fixed once instead of every boot.

- `prune` introduces **no new predicate**: its candidates are exactly the orphans `spec/06` defines and `volume list` surfaces. That is what lets it join a family with no `create` — declarative removal produces orphans on purpose, as a recovery buffer, and `prune` is the batch half of that lifecycle.
- **It never removes a declared volume, under any flag. There is no `--all`.** The prior art that grew one is also the prior art whose scope is still misread.
- Safety rides existing rails: it refuses while the VM runs, prompts on a TTY, takes the same confirmation flag `destroy` does, and prints the reclaimed measurement.
- **First-boot ownership is declarative and touches only the mount point**, applied after the volume mounts, with a marker inside the volume so a later boot never re-seeds over user data. A recursive fix would be proportional to a home reaching tens of gibibytes, and would overwrite ownership set deliberately.

## Consequences

- Good: the orphan flag becomes actionable in one step.
- Good: a fresh home is usable on first boot without an imperative boot script.
- Bad: one more verb in a surface that resists them, and one more row in the exit-code matrix.
- Bad: a user who wanted "remove everything" must still reach for `destroy`.

## Status

Accepted

Amends [`ADR-0019-volume-model.md`](./ADR-0019-volume-model.md) — the lifecycle surface gains `prune`; the declarative-only creation rule, N18, and the rest of the model are unchanged.

Extends [`ADR-0048-guest-module-only-vivarium-owns-the-runner.md`](./ADR-0048-guest-module-only-vivarium-owns-the-runner.md) — first-boot home initialization is the guest module's, like everything else in the guest.

Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
