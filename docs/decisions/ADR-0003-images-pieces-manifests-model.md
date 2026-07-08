# ADR-0003: Model composition as images, pieces, and manifests

## Context and Problem Statement

Users need to build many related sandboxes without copying configuration. A Rust project and a
Python project share a base VM and cross-cutting concerns (git identity, ssh-agent forwarding, cache
mounts) but differ in toolchain. The model must let users reuse coarse bases and fine-grained
fragments, and bind a project to one authoritative composition.

## Considered Options

- **Monolithic per-project config** — each project writes one complete sandbox definition.
- **Base + overrides only** — a single inheritance chain of whole VM definitions.
- **Three artifact kinds — images, pieces, manifests** — coarse VM bases, small reusable config
  fragments, and a unifier that names which image and pieces combine.

## Decision Outcome

Chosen option: **images, pieces, and manifests**. An **image** is a composable VM base (for example a
minimal base, or a Rust or Python toolchain). A **piece** is a small config fragment layered onto an
image (for example ssh-agent forwarding or an egress allowlist). A **manifest** is the unifier: it
names one image plus an ordered set of pieces and is the single source of truth a project binds to.

This mirrors how projects actually vary: images capture the toolchain, pieces capture cross-cutting
concerns, and manifests capture the per-environment combination. See
[`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) for the shapes and
examples.

## Consequences

- Good: maximal reuse — one piece serves every manifest that imports it.
- Good: a project points at exactly one manifest, giving a clear single source of truth.
- Good: maps cleanly onto module `imports` (see
  [`ADR-0002-module-system-as-composition-engine.md`](ADR-0002-module-system-as-composition-engine.md)).
- Bad: three concepts to learn instead of one.
- Bad: requires discipline to keep pieces small and single-purpose.

## Status

Accepted
