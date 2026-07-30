# ADR-0057: The manifest grammar is a closed key table with one validation boundary

## Context and Problem Statement

[`ADR-0047-manifest-carries-no-schema-version.md`](./ADR-0047-manifest-carries-no-schema-version.md) settled that an unknown key fails closed, but nothing enumerated the accepted keys: [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md) offers an illustrative example, and the shapes are scattered across ADR-0019, ADR-0020, and ADR-0021. Nor was it settled which defects are parse errors and which are evaluation errors — so one defect could plausibly carry two exit codes.

## Considered Options

- Keep the example as the authoring surface.
- Publish an exhaustive key table only.
- Publish the key table **and** a parse-versus-evaluate validation boundary.

## Decision Outcome

Chosen option: **key table plus validation boundary** — an unknown-key rule is only as good as the list of known keys, and a code is only stable if one defect never has two.

- The exhaustive table lives in `spec/03`. `image` stays the only required key; every other table is optional, and absent is never the same as empty.
- **Units live in key names** — `mem_mib`, `size_gib` — never in value suffixes. A suffix grammar re-imports the 1000-versus-1024 ambiguity and buys nothing TOML's integers lack.
- **`size_gib` joins the volume shape.** `spec/17` already publishes a 32 GiB default and `spec/06` already says a ceiling may be raised between boots, but no key expressed it.
- **No `resources.disk`.** Disk belongs to a volume, which already reports allocated-against-virtual per volume.
- **The boundary:** decidable from the manifest text alone — syntax, unknown key, wrong type, out-of-domain value, unresolvable name — is `78` at parse. Decidable only after the layers merge is `65` at evaluation. Duplicate volume names and duplicate mount targets are checked **once, at evaluation**, even though a single manifest can violate them alone; splitting them would give one defect two codes, which `spec/14`'s permanent-API rule forbids.

## Consequences

- Good: the grammar becomes a lookup table, and the generated example and JSON Schema (ADR-0012) gain an exhaustive source to reflect.
- Good: one defect, one exit code.
- Bad: an intra-manifest duplicate is reported after evaluation rather than at parse, which is later than it could be.
- Bad: the additive-grammar promise now carries visible bookkeeping — every new key is a documented row.

## Status

Accepted

Amends [`ADR-0019-volume-model.md`](./ADR-0019-volume-model.md) — the volume shape gains `size_gib`, the key for the virtual ceiling ADR-0037 made raisable. The model is otherwise unchanged.

Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md), and [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).
