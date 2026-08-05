# ADR-0061: vivarium ships examples, never a second resolution namespace

## Context and Problem Statement

Whether vivarium ships stock images and pieces was never settled. [`ADR-0012-generate-config-examples-from-types.md`](./ADR-0012-generate-config-examples-from-types.md) already says Nix-module artifacts "ship as hand-maintained example modules under the same copy-don't-scaffold discipline", which answers most of it — but leaves open whether a bundled library also resolves by name, and it does not distinguish those optional examples from the tool-owned module a guest cannot boot without.

## Considered Options

- Ship nothing; the user authors every image and piece.
- Ship a bundled library that resolution falls back to behind the user's config root.
- Ship examples only; the resolution path stays the config root alone.

## Decision Outcome

Chosen option: examples only.

- Resolution stays exactly ADR-0045: one config root, flat form then directory form, both spellings present fails closed. A bundled fallback would need a shadowing rule, a provenance field `viv images list` does not have — its rows are `{name, path}` — and a perpetual compatibility obligation for every bundled module, all to avoid one `cp`.
- What ships to copy: the generated annotated manifest example (ADR-0012), one base image example, and a small set of single-concern pieces. They live in this repository, never installed into a config root (N13).
- What is not an example: the tool-owned `vivarium.*` options module and the guest wiring vivarium owns (ADR-0021, ADR-0041, ADR-0048). That module is non-optional, ships inside the generated flake (ADR-0058), is never copied, and never appears in a library listing. Confusing the two is what made this question look open.
- The data root already reserves the slot for a pinned external module library if one is ever wanted; that is deliberately not built now.

## Consequences

- Good: one namespace, so `viv images list` stays `{name, path}` and a name has exactly one meaning.
- Good: copy-don't-scaffold finally has something to copy.
- Bad: a first run requires a copy step before anything builds.
- Bad: the examples age against upstream with no pin of their own, so they are documentation that can rot.

## Status

Accepted

Amends [`ADR-0012-generate-config-examples-from-types.md`](./ADR-0012-generate-config-examples-from-types.md) — names what the hand-maintained example modules concretely cover, and separates them from the non-optional tool-owned module, which is not an example and is never copied.

Amends [`ADR-0045-config-root-library-layout-and-name-resolution.md`](./ADR-0045-config-root-library-layout-and-name-resolution.md) — the config root is the whole search path; a bundled artifact never participates in name resolution.
