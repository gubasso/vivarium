# ADR-0103: the published deck is its own presentation tree

## Context and Problem Statement

vivarium's documentation lives in five reader-need zones under `docs/`, gated by a Markdown regime that forbids emphasis, unwraps every paragraph onto one physical line, and requires a single top-level heading per document. A Slidev deck published to GitHub Pages breaks all three by construction: slides are separated by `---`, each opens with its own front matter and its own heading, and a slide carries weight where prose carries a sentence. The regime does not merely complain. `textWrap: "never"` pulls the line above a `---` onto it, CommonMark reads the pair as a setext heading, and the deck then builds wrong with no diagnostic anywhere.

## Considered Options

- A sixth `docs/` zone holding the deck, exempted from the Markdown hooks
- A top-level `slides/` tree outside the documentation zones, exempted by directory
- A deck written to survive the existing regime, with no exemption

## Decision Outcome

Chosen option: `a top-level slides/ tree` — the five zones classify reader need and a deck answers none of them, so presentation source belongs outside the documentation tree rather than widening what that tree means.

Slidev comes from npm, pinned by a committed `slides/package-lock.json`, on the Node the devShell already supplies for markdownlint. Nix owns the runtime; npm owns the tool. The exemptions are directory-scoped and live beside the hooks they suspend, in [`../../.pre-commit-config.yaml`](../../.pre-commit-config.yaml).

The deck is the whole published site, served at the project-page root. A renderer for `docs/` is out of scope.

## Consequences

- Good: one directory carries the deck, its pin, and its exemptions, and `docs/` keeps its five-zone rule intact.
- Good: the deck's only gate becomes whether it builds, which is the failure a prose linter could never have seen.
- Bad: `slides/` is a fourth top-level kind beside `nix/`, `tests/`, and `scripts/`, so the repository's directory story gains a category.
- Bad: an exemption granted by directory cannot notice documentation prose that drifts into `slides/`.

## Status

Accepted
