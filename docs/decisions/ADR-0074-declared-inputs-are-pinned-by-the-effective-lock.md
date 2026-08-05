# ADR-0074: Declared inputs are pinned by the effective lock, and a missing node refuses

## Context and Problem Statement

[`ADR-0073`](./ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md) lets a shared artifact declare a flake input but deliberately gives the declaration no revision. Something must say where the pin comes from, what `viv update <input>` names, and what happens when the lock in force carries no node for a declared input — a case a team override lock makes ordinary, since adopting a piece that declares a new input does not change the shared lock.

## Considered Options

- Resolve a missing node during the build, then record it
- Refuse the build, naming the lock in force
- Give the declaration its own revision, so it needs no lock node

## Decision Outcome

Chosen option: refuse — resolving behind the user is the unannounced input jump N3 exists to prevent, and a second pin would break "exactly one effective lockfile".

Declared inputs are ordinary nodes in the one effective lock: the team override lock beside the manifest when present, otherwise the tool-owned per-target lock. No second pin file, and no revision in the declaration.

`viv update <input>` names a declared identifier verbatim, and that same string is the `name` in the update record. An unknown name stays `64`.

When the effective lock carries no node for a declared input, `start` and `config eval` fail closed at `78`, naming the input, the artifact that declares it, and the lock in force. Under the tool-owned lock the remedy is `viv update`. Under a team override lock there is none inside vivarium: `viv update` still refuses ([`ADR-0062`](./ADR-0062-override-lock-is-per-manifest-and-update-refuses.md)), and moving the shared pin is the team's own act.

## Consequences

- Good: exactly one effective lockfile stays literally one, and no ordinary build re-resolves.
- Good: the refusal names the asymmetry, so a user under a team lock learns the remedy is outside vivarium rather than retrying.
- Bad: adopting a piece that declares a new input is a two-step act — adopt, then update — and under an override lock the second step needs someone else.
- Bad: it costs a soft `lock-covers-declared-inputs` check so the gap surfaces before the command that hits it.

## Status

Accepted

Specified in [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md), [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md), and [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
