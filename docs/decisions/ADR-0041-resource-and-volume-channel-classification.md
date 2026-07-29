# ADR-0041: Channel classification of resources and volumes

## Context and Problem Statement

ADR-0021 named `vivarium.mounts` and `vivarium.env` as the tool-owned option surface, and N19 enumerates mounts and runtime environment as the launch channel. But [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) shows a **piece** setting `resources.mem_mib`, and [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) plus ADR-0019 say pieces contribute `[[volumes]]`. Neither has an option path, so neither is expressible — and it was never settled which channel they belong to.

## Considered Options

- Leave both manifest-only; pieces cannot contribute either.
- Name both as launch-channel options, matching the existing `vivarium.*` shape.
- Classify each by what actually depends on it.

## Decision Outcome

Chosen option: **classify each by what depends on it** — the channel split is a purity rule, not a naming convention.

- **`vivarium.resources.*` is launch-channel.** [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md) already fixes ceilings as launch-time values that two hosts resolve differently from the same build, and ADR-0035 makes the ceiling a monitor launch argument. N19's enumeration widens to admit them.
- **`vivarium.volumes` is build-channel, materialized at launch.** A volume's guest mountpoint is guest system configuration, so a build output depends on it; calling it launch-channel would claim that adding a volume needs no rebuild and would invert N19. Only the host image path and virtual size resolve at launch.
- Two guards keep a declared ceiling out of the build: the balloon-driver assertion covers the **driver**, never the figure; and the guest's compressed swap device sizes from RAM observed at boot, never from the declaration.

## Consequences

- Good: a piece is whole for resources too, and the command surface's own tie example becomes expressible.
- Good: N19 stays literally true rather than quietly incomplete.
- Bad: `vivarium.*` no longer means "launch channel" — the split is per-option, so ADR-0021's title over-generalizes.
- Bad: a volume contributed by a piece triggers a rebuild, which may surprise someone who expects volumes to be pure runtime.

## Status

Accepted

Amended by **ADR-0049** — the first of the two guards fires at evaluation rather than as a `doctor` probe, because the guest kernel it inspects is one vivarium builds. The guard itself is unchanged: it still covers the driver and never the declared figure.

Amends [`ADR-0021-typed-launch-channel-options-in-pieces.md`](./ADR-0021-typed-launch-channel-options-in-pieces.md) and [`ADR-0019-volume-model.md`](./ADR-0019-volume-model.md). Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md), [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md), [`../reference/spec/09-glossary.md`](../reference/spec/09-glossary.md), and [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).
