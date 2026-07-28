# ADR-0006: Bind a project to one manifest with a fixed resolution precedence

## Context and Problem Statement

Each project selects exactly one manifest, but the selection can come from several places: an explicit flag, a committed file in the repository, a personal override, or a per-user registry that keeps the repository clean. Without a single, predictable precedence, the effective manifest becomes ambiguous and non-reproducible across machines.

## Considered Options

- **Repository file only** — the manifest is always named by a committed file in the repo.
- **User registry only** — a per-user map from project path to manifest, nothing in the repo.
- **Both sources with a fixed precedence** — support a committed repo pointer, a gitignored personal override, and a user registry, resolved in one defined order.

## Decision Outcome

Chosen option: **both sources with a fixed precedence**, resolved highest-wins: explicit CLI flag → environment variable → repository personal override (gitignored) → repository committed pointer → user-registry entry for the project path → configured default → otherwise **fail closed** with a message to initialize the project.

This lets a team commit a shared pointer while an individual overrides it locally, and lets a user keep a repo pristine by binding in the registry instead. See [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) for the binding files and registry shape.

## Consequences

- Good: the effective manifest is always deterministic and explainable.
- Good: supports both team-shared and repo-clean personal workflows.
- Good: fail-closed avoids silently booting an unintended sandbox.
- Bad: several resolution sources to document and teach.
- Bad: a personal override can mask the committed pointer, so "works for me" drift is possible.

## Status

Superseded

Superseded by [`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md), which drops the per-project pointer files and the default-manifest fallback, moves the registry to the state root, and collapses the precedence to `--manifest` → `VIVARIUM_MANIFEST` → registry → fail closed.
