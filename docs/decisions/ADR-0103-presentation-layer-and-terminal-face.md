# ADR-0103: Presentation layer and terminal face

## Context and Problem Statement

[`ADR-0015`](./ADR-0015-cli-output-and-failure-contract.md) fixes the stream split, the color chain, and TTY-only progress, but nothing implements them: no color, no progress, no `--help`, no `-v`/`-q` exist, and `nix build` runs silent for minutes. The question is what supplies the terminal face — and where the boundary sits between borrowed rendering and owned contract.

## Considered Options

- Hand-rolled, zero new dependencies: first-party SGR, spinner thread, tables.
- `anstyle`/`anstream` plus `indicatif`: the rust-cli working-group stack.
- `cliclack` over `indicatif` and `console` for stderr; `console` styling plus first-party tables for stdout.
- A Rich-for-Rust port (`rich-rs` and siblings).

## Decision Outcome

Chosen option: `cliclack for stderr, console plus first-party tables for stdout` — the library split follows the stream split the contract already fixes.

cliclack writes to stderr, exactly the channel spec/01 gives progress, prompts, and warnings; it builds on `indicatif` and `console` rather than competing, and its `Theme` trait keeps the look ours. stdout stays plain data: `console::Style` colors it, and table layout is first-party because every table has fixed, known columns. The Rich ports are toys by adoption; `miette` would re-render a skeleton [`ADR-0075`](./ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md) froze.

What stays owned regardless of crate: the `NO_COLOR > FORCE_COLOR > isatty` chain, because no crate implements that exact order; the diagnostic skeleton, rendered once and parameterized by a palette so the colored and plain forms are one text; and the suppression rule, held in one resolver so no command asks whether it is quiet. Styles are always forced or absent — never left to a crate's own global detection.

## Consequences

- Good: the spinner and color degradation come proven; every published shape stays first-party.
- Good: progress hides itself off a TTY by the face's construction, not per-command discipline.
- Bad: cliclack is young beside its foundations; its `Theme` seam is the contained blast radius.
- Bad: two styling idioms exist — `cliclack::Theme` on stderr, `console::Style` on stdout.

## Status

Implemented

Enacted by [slice 016](../plan/slices/016-the-human-face/README.md): `src/ui/` carries the chain, the palette, the watcher, and the theme; the four buffered child runs stream; `--help`, `--version`, and the global `-v`/`-q` exist.
