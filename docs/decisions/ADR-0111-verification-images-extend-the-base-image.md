# ADR-0111: Verification images extend the base image

## Context and Problem Statement

vivarium builds one guest image; verification builds variants of it to measure what the shipped one deliberately cannot do. Two problems accumulated. The shipped artifact is called `first-microvm`, a name recording when it was first built rather than what it is, and every variant inherits the noun. And while [`../../nix/default.nix`](../../nix/default.nix) composes each variant by appending modules to `mkImage`, nothing asserts it stays that way: a variant that replaced rather than appended would evaluate cleanly, and the check running against it would measure the replica rather than the product.

## Considered Options

- Rename the artifact to `base-image` and assert the extension relationship in an evaluation-tier check.
- Keep the name and add the check alone.
- Record the extension rule as prose in the verification root and rely on review.

## Decision Outcome

Chosen option: `rename plus check` — a name saying "the thing everything else extends" makes the rule readable, and a check makes it true. `packages.base-image` is the shipped artifact; variants are `base-image-measurement`, `base-image-scaled`, `base-image-agent-check` and `base-image-bench-threads-N`. A check asserts that each variant's settings differ only on keys it declared, and that every base share and volume survives with its content rather than merely its name. [`ADR-0095`](./ADR-0095-measurement-services-live-in-a-measurement-image.md) put the probes in a variant; this keeps that variant honest.

## Consequences

- Good: an image that replicates rather than extends fails at `nix flake check`, before it can measure itself.
- Good: the name states the relationship, so a reader meets the rule before it is explained.
- Bad: published flake attribute names change, and every lane and living document naming the old one moves with them.
- Bad: the launcher binary becomes `vivarium-base-image` while remaining a launcher rather than an image.
- Bad: units are compared by name only. Every variant legitimately changes some unit — appending a tree reorders `vivarium-agent`, declaring store thresholds rewrites `nix-daemon` — so comparing content would fail on every image. A module that force-rewrites a base unit under its own name is not caught; recorded rather than implied otherwise.

## Status

Implemented

Enacted by [slice 030](../plan/slices/030-the-base-image-is-the-base/README.md). The rename is in [`../../nix/`](../../nix), [`../../tests/nix/`](../../tests/nix) and the host lanes; the rule is executable as `checks.base-image-extends`, built by [`../../tests/nix/extension.nix`](../../tests/nix/extension.nix).
