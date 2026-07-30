# ADR-0063: `extends` requires the directory manifest form

## Context and Problem Statement

[`ADR-0060-extends-is-one-local-module.md`](./ADR-0060-extends-is-one-local-module.md) copies the `extends` module's containing directory into the generated flake so its relative imports work. [`ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](./ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) excludes `manifests/` from what is materialized. For a flat manifest those are the same directory — and the example in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) uses the flat spelling. Two accepted ADRs are therefore binding and inconsistent.

## Considered Options

- Copy only the named `.nix` file, forbidding relative imports from it.
- Copy the module plus a closure computed from its `imports`.
- Require the directory manifest form whenever `extends` is present.

## Decision Outcome

Chosen option: **require the directory form.** A manifest naming `extends` must resolve as `manifests/<name>/default.toml`; the target must canonicalize, after symlinks, inside that same directory; and the copied unit is exactly `manifests/<name>/`.

- **N4 holds exactly.** The copied unit carries no other manifest and no other project's helpers, so an unrelated manifest edit cannot change this project's output path — which is ADR-0058's whole rationale for materializing.
- **The directory form already exists for this.** ADR-0045 introduced it so a multi-file artifact can keep helper modules beside it. `extends` is that case, not a new concept.
- **Resolution narrows** from "inside the config root" to "inside the manifest's own directory". Nothing is lost: ADR-0060 already rules that a shared baseline is a **piece**, so a shared `extends` module contradicts its own doctrine.
- A flat manifest naming `extends` is **`78` at parse**, naming the required spelling — ADR-0045's ambiguity idiom, which one `mv` resolves.
- **A computed closure is rejected**: `imports` accepts arbitrary module expressions, not only literal paths, so it is not enumerable without evaluating — and evaluation needs the tree materialized first.
- **A single-file copy is rejected**: ADR-0058 rejected per-file copying already, and a broken relative import would surface as a build fault (`70`) rather than the parse-time `78` the hatch promises.

## Consequences

- Good: the contradiction closes, and the `manifests/` exclusion gains one bounded exception.
- Good: the escape hatch's blast radius is one directory.
- Bad: reaching for `extends` costs one `mv`.
- Bad: a spelling that was free choice now carries a rule.

## Status

Accepted

Amends [`ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](./ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — `manifests/` is still never copied wholesale, but the selected manifest's own directory is materialized when it names `extends`.

Amends [`ADR-0060-extends-is-one-local-module.md`](./ADR-0060-extends-is-one-local-module.md) — the target must canonicalize inside the manifest's own directory rather than anywhere in the config root, and the flat manifest form is rejected for a manifest that names `extends`.

Amends [`ADR-0045-config-root-library-layout-and-name-resolution.md`](./ADR-0045-config-root-library-layout-and-name-resolution.md) — the directory form stays a fallback everywhere except for a manifest naming `extends`, where it is required.

Specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), and [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md).
