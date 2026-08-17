# ADR-0102: The installation supplies vivarium

## Context and Problem Statement

vivarium is installed however its user chooses, and nothing else may supply it. Slice 012 nonetheless made the generated project flake declare the product flake as a third baseline input beside `nixpkgs` and `microvm`, so a project build fetched vivarium's own Nix half, built the tool from source, and the launcher execed that copy rather than the program the user invoked.

A program cannot depend on itself. The dependency is also unsatisfiable: the input is locked, so the copy may be older than the tool running, and both halves then agree with each other while disagreeing with the installation — which is how one launch reported success against a record the tool could not read.

## Considered Options

- Keep the input, pinned to a version derived from the running binary.
- Keep the input, resolved live from a branch.
- Remove the input; the binary carries its own Nix half.

## Decision Outcome

Chosen option: `remove the input` — an installation is sufficient by definition, so there is nothing to name, fetch, pin, or reconcile.

The binary carries the product Nix tree and writes it into each generated tree, referenced relatively, as with `vivarium-options.nix` and `vivarium-report.nix`. It is written as source, so it still evaluates against the project's own `nixpkgs` and `microvm`. No host-side vivarium program comes from a project build. The guest agent stays built — it runs inside the guest — and its handshake refuses a version it cannot speak.

The effective lock pins upstream and artifact-declared inputs, never vivarium. State an earlier vivarium wrote is stale by construction, and is shed rather than read.

## Consequences

- Good: the installed `viv` is the only vivarium that runs, under any package manager.
- Good: a skew between the tool and its own guest description cannot be assembled, so it needs no detection.
- Bad: the tool version becomes a declared build input, so upgrading `viv` makes an existing build stale.
- Bad: locks from an earlier vivarium carry a dead node, needing a migration that sheds it without moving a surviving pin.

## Status

Implemented

Enacted by [slice 015](../plan/slices/015-the-binary-supplies-itself/README.md). Amends the determinism guarantee in [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md), which named the manifest closure and the lockfile alone. Rides the regeneration seam [`ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](./ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) fixes, and leaves [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) untouched for the inputs that remain.
