# ADR-0012: Generate config examples from types; copy-don't-scaffold

## Context and Problem Statement

Users must author their own config (ADR-0011: the tool never writes it). But hand-written docs and inline examples drift from the real config types, and a tool that scaffolds starter files would violate the read-only-config rule. We need a way to give users accurate, current, self-documented config to copy — without the tool ever writing into their config root.

## Considered Options

- **Scaffold on `init`** — the tool writes a starter config file. Rejected: violates P1 (ADR-0011).
- **Hand-maintained example docs** — example files written and updated by humans. Drifts silently from the types.
- **Generate examples from the config types** — derive annotated examples and a schema from the tool's own types, checked for freshness in CI/pre-commit.

## Decision Outcome

Chosen option: **generate examples from the config types** (P3 — copy-don't-scaffold).

- The TOML config surfaces (global config, manifest) derive from the Rust config types via serde + `schemars`. The generator emits a JSON Schema (for editor validation) and an annotated `*.example.toml` — required keys active, optional keys commented, placeholder values, and a header stating "copy this; the tool never writes your config."
- Freshness is enforced by a **pre-commit `--check`** hook that regenerates and fails on drift, so the committed examples always match the current types.
- Nix-module artifacts (images, pieces) cannot be reflected from Rust types; they ship as hand-maintained example modules under the same copy-don't-scaffold discipline.
- Users copy an example into their config root and edit it. The tool only ever reads the result.

## Consequences

- Good: examples never drift; users get accurate, self-documenting config; config stays read-only to the tool.
- Good: the JSON Schema doubles as editor/CI validation.
- Bad: adds a generator and a pre-commit gate to maintain; Nix examples stay manual.

## Status

Accepted

Amended by [`ADR-0061-examples-ship-not-a-second-namespace.md`](./ADR-0061-examples-ship-not-a-second-namespace.md) — names what the hand-maintained example modules cover, rules that none of them ever participates in name resolution, and separates them from the non-optional tool-owned options module, which is not an example and is never copied.
