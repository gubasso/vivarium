# ADR-0004: Author manifests as TOML that compiles to a generated flake

## Context and Problem Statement

A manifest is the unifier a project binds to (see [`ADR-0003-images-pieces-manifests-model.md`](./ADR-0003-images-pieces-manifests-model.md)). Its authoring format must be simple to write, diff, and grep, while the actual composition is still performed by the module system. Requiring users to write Nix for every manifest raises the barrier and mixes authoring with the merge mechanism.

## Considered Options

- **Manifest as a raw Nix module** — the user writes `imports = [ ... ]` directly.
- **Manifest as TOML compiled by the tool** — a thin declarative name-list that the CLI turns into a generated flake whose `imports` are the named image and pieces.

## Decision Outcome

Chosen option: **manifest as TOML compiled to a generated flake**. The manifest is a name-list (`image = "rust"`, `pieces = [ ... ]`, resource and egress knobs); the CLI resolves those names to module files and emits a flake that imports them, then builds it. The module system still does all merging — the CLI only assembles the `imports` list.

For compositions the TOML cannot express, a manifest may reference a raw `.nix` module as an escape hatch. See [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) for the manifest schema.

## Consequences

- Good: authoring is a readable, greppable name-list; no Nix fluency required for common cases.
- Good: the tool implements only translation, not merging — the "boring part."
- Good: power users retain full Nix expressiveness through the `.nix` escape hatch.
- Bad: two formats to understand (TOML manifest and the modules it names).
- Bad: the generated flake is an intermediate artifact users may need to inspect when debugging.

## Status

Accepted

Amended by [`ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](./ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — the generated flake's location, contents, and inspection path are fixed there, closing the "intermediate artifact users may need to inspect" consequence above.

Amended by [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — the flake the CLI emits reads a lockfile it does not own, staged in from the data root.

Amended by [`ADR-0060-extends-is-one-local-module.md`](./ADR-0060-extends-is-one-local-module.md) — the raw-`.nix` escape hatch sanctioned above is exactly one local module, and never inheritance of another manifest.
