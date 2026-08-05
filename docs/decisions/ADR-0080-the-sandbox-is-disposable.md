# ADR-0080: The sandbox is disposable

## Context and Problem Statement

Nothing in the goals says what a sandbox is worth. Without that, every gap in guest-side durability reads as a missing feature — snapshots, rewind, backup — and each one is individually arguable. The result is a slow drift toward a production runtime with safeguards, which is not what vivarium is for.

## Considered Options

- Leave durability unstated and judge each request on its merits
- State disposability as a goal, and derive the refusals from it
- Provide guest-side data protection (checkpoints, backup integration)

## Decision Outcome

Chosen option: state it as a goal — a principle answers a class of requests once, where a feature list answers them one at a time.

A sandbox is disposable: it can be destroyed and rebuilt at any time, and nothing of value is lost. That is affordable because the valuable things are elsewhere by construction — the work is in the workspace, which is the user's own version-controlled directory on the host and is never vivarium's to lose (N9); and the environment is in the manifest, which reproduces the same VM anywhere (N3).

Guest-side state — the default volume, caches, whatever the guest wrote outside the workspace — is therefore continuity, not a system of record. Volumes persist so a warm restart is cheap, not so data is safe. That vivarium never removes them implicitly (N18) is a constraint on the tool, not a durability promise to the user.

The refusals follow rather than being argued separately: no snapshot or restore, no checkpoint or rewind, no backup integration, no repair of a damaged guest. `viv destroy` and `viv start` are the recovery path, and their cost is a rebuild.

Guest-side protection was rejected because it inverts the value proposition: the mechanisms that make a sandbox worth preserving are exactly the ones that make it precious to maintain.

## Consequences

- Good: a whole class of feature requests has one answer, and the answer is already true.
- Bad: a user who keeps unique data only inside a guest can lose it, and no vivarium feature will help.

## Status

Accepted

Specified in [`../reference/spec/00-goals-and-non-goals.md`](../reference/spec/00-goals-and-non-goals.md) and [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md).
