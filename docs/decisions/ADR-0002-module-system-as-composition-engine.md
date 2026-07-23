# ADR-0002: Use the NixOS module system as the composition engine

## Context and Problem Statement

vivarium composes a sandbox from many layers — a base VM, language toolchains, and small config
fragments. Something must merge those layers into one coherent configuration with predictable rules
for overrides, list accumulation, and conflicts. Writing and maintaining a bespoke merge engine is
costly and error-prone.

## Considered Options

- **Custom merge engine** — define an ordered "last layer wins" stack with hand-written rules for
  scalars, lists, and maps.
- **The NixOS module system** — express every layer as a NixOS module and let the module system
  merge them: `imports` collects layers, lists auto-concatenate, and scalar conflicts resolve by
  priority (`lib.mkDefault` / normal / `lib.mkForce`).

## Decision Outcome

Chosen option: **the NixOS module system** — because vivarium builds NixOS microVMs, the module
system is already present and is a mature, well-specified merge engine. Reusing it deletes the entire
custom-merge surface.

Merge is **priority-based, not order-based**: a base layer sets `mkDefault`, a project leaf uses
normal priority, and a hard floor uses `mkForce`. Lists such as package sets and mount lists
concatenate across layers. See
[`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md)
for the full semantics.

## Consequences

- Good: no merge engine to build, test, or maintain; behavior is battle-tested.
- Good: the whole nixpkgs module ecosystem is reachable as composable layers.
- Good: more expressive than last-wins — hard floors and soft defaults are first-class.
- Bad: authors must learn priority-based merge, which differs from intuitive ordered layering.
- Bad: ties the composition model to Nix; a non-Nix backend could not reuse it.

## Status

Accepted
