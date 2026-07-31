# ADR-0073: Shared artifacts declare their own flake inputs

## Context and Problem Statement

The manifest key table is closed and carries no `inputs` key, yet `viv update [<input>...]` is a specified verb and [`ADR-0072`](./ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md) makes "import a NixOS module" the sanctioned route to encrypted-at-rest. A shared piece needing a third-party module library has no way to say so. A NixOS module is not a flake and carries no inputs of its own; only a root flake can — and vivarium generates the root flake.

## Considered Options

- Declare inputs in the manifest
- Declare inputs in the piece or image
- Resolve inputs by convention from a directory the user populates

## Decision Outcome

Chosen option: **declare in the piece or image** — the artifact that needs a dependency is the artifact that names it, so adopting it stays a one-file act.

An image or piece declares inputs in an `inputs.toml` beside it, which requires the directory form. Each entry gives a `url` — a flake reference — and an optional `flake = false`. It carries **no revision**: pinning belongs to the lockfile alone (ADR-0074). Identifiers use the config-root name grammar, and vivarium's own baseline input names are reserved.

vivarium unions the declarations across the resolved image and ordered pieces into the generated flake's `inputs`, passing them to every layer as `vivariumInputs.<name>`. Identical declarations of one name coalesce; two artifacts declaring one name differently is a resolve-stage defect at `78` naming both.

The manifest was rejected as the personal layer: a piece that cannot carry its own dependency stops being whole and shareable. Convention was rejected as ambient state nothing pins and `viv update` cannot name.

## Consequences

- Good: a piece stays self-contained, and the dependency graph is known before evaluation rather than discovered mid-build.
- Good: a flake reference is neither a host path nor personal data, so N11 is untouched.
- Bad: a second authored format sits beside `default.nix`, and the directory form becomes mandatory for any artifact declaring an input.
- Bad: the declaration lands in the store with the artifact, so a reference embedding a credential is a build-time secret (N10).

## Status

Accepted

Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), and [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md). Pinning and the missing-node refusal are decided separately in [`ADR-0074`](./ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md).
