# ADR-0040: The manifest is the personal layer

## Context and Problem Statement

[`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md) classified images, pieces, and the manifest as shared. But N7 gives a project one manifest with no overlay, and priorities can only be set from `.nix` — so a personal override must be a piece, and a piece enters the merge only through that shared manifest's `pieces` list. Any per-user tweak meant editing the file the team shares. The priority convention had no personal tier either.

## Considered Options

- Record extra pieces on the state-root binding (`--piece`, `VIVARIUM_PIECES`).
- Auto-import a well-known personal piece keyed by `<project-id>`.
- Reclassify: share images and pieces; the manifest is each user's own.

## Decision Outcome

Chosen option: reclassify — it matches where manifests already live (the per-user config root) and needs no new mechanism at all.

- Guarantees that must hold for everyone live in images and pieces, where `mkForce` makes them unwaivable. The manifest carries only per-user choices: which image and pieces to adopt, resource ceilings, mounts, env.
- No fourth priority tier. The manifest leaf already sits at normal priority — above image and piece defaults, below a `mkForce` floor. The personal layer was missing an entry point, not a rank.
- Shared pieces propose with `mkDefault`; only floors use `mkForce`. Two pieces setting one scalar at normal priority collide, and the manifest must be able to decide.
- A team distributes an example manifest and each user copies it (ADR-0012). Publishing a manifest for reference stays social; no import path depends on it.
- N11 narrows: literal personal paths are forbidden in images and pieces, legal in your own manifest.

## Consequences

- Good: no new flag, binding-schema change, or priority tier; one manifest per project (N7) is untouched.
- Good: N11 finally names a personal layer that exists.
- Bad: no single file pins a team baseline, so drift is visible only on inspection until `extends` is designed.
- Bad: reproducibility now rests on piece discipline rather than on one shared file.

## Status

Accepted

Amended by [`ADR-0060-extends-is-one-local-module.md`](./ADR-0060-extends-is-one-local-module.md) — the consequence "no single file pins a team baseline … until `extends` is designed" is answered by a shared piece, which after ADR-0021 and ADR-0041 carries everything a manifest expresses except the image line and the piece list. `extends` is a single local module and never inherits another manifest, so it is not that mechanism. The reclassification above is unchanged.

Amends [`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md) and [`ADR-0021-typed-launch-channel-options-in-pieces.md`](./ADR-0021-typed-launch-channel-options-in-pieces.md). Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md), [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md), and [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md).
