# ADR-0060: `extends` is one local module, not manifest inheritance

## Context and Problem Statement

`extends` is declared in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) and rendered by `viv manifest show`, but it has no semantics — path resolution, purity, arity, and merge position are all undefined. [`ADR-0040-manifest-is-the-personal-layer.md`](./ADR-0040-manifest-is-the-personal-layer.md) names it as the missing mechanism for a live team baseline, which invites reading it as manifest-to-manifest inheritance.

## Considered Options

- TOML-to-TOML manifest inheritance: a manifest extends another manifest.
- A single raw `.nix` module.
- An ordered array of modules, resolved transitively.

## Decision Outcome

Chosen option: **a single raw `.nix` module**, and manifest inheritance is rejected outright.

- **Resolution** is relative to the manifest file's own directory; the result must canonicalize, after symlinks, to a path inside the config root. An escape, an absent file, or a non-module target is `78` at parse.
- **Exactly one, never transitive.** A Nix module already has `imports`, evaluated by the module system (N6); ordering several `extends` entries ourselves would resurrect the declaration-order tiebreak ADR-0042 deliberately removed.
- **Its containing directory is copied into the generated flake** (ADR-0058), so it is a store input, its own relative imports work, and N3 holds.
- **It merges at the same rank as a piece** and chooses its own priority. A shared baseline proposes with `mkDefault`; `mkForce` is what `spec/01`'s tie hint means by "override through extends".
- **Manifest inheritance is rejected** because it requires publishing, per key, whether a child value replaces or merges with its parent's. That table is precisely the bespoke merge implementation N6 forbids, and it would stand a second priority ladder beside the module system's.
- **The team baseline is a shared piece.** After ADR-0021 and ADR-0041 a piece carries packages, guest config, mounts, env, resources, and volumes, and may import an image — so it is a genuinely live baseline. The residual drift is the `image` line and the `pieces` list.

## Consequences

- Good: the escape hatch gets a boundary, and ADR-0040's open gap closes with no new mechanism.
- Good: no merge semantics enter vivarium.
- Bad: `extends` cannot inherit another manifest, which is what the name suggests.
- Bad: a raw module can `mkForce` past every convention — the hatch is genuinely sharp.

## Status

Accepted

Amends [`ADR-0040-manifest-is-the-personal-layer.md`](./ADR-0040-manifest-is-the-personal-layer.md) — its "no single file pins a team baseline … until `extends` is designed" is answered by a shared **piece**, not by `extends`. That consequence is now closed; the reclassification it records is unchanged.

Amends [`ADR-0004-toml-manifest-compiles-to-flake.md`](./ADR-0004-toml-manifest-compiles-to-flake.md) — the raw-`.nix` escape hatch it sanctioned is bounded here.

Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) and [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md).
