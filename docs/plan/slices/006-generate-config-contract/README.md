<!-- markdownlint-configure-file {"MD043": {"headings": ["?", "## Goal", "## Appetite", "## Core", "## In scope", "## Out of scope", "## Governed by", "## Acceptance", "## Rabbit holes", "## Done when", "## Revisions"], "match_case": true}} -->

# 006 — Generate the configuration contract

## Goal

Make the Rust type model generate the JSON Schema, annotated examples, and specification default column from one source.

## Appetite

2 implementation sessions.

## Core

One generator emits schema and examples, and a `--check` mode fails on stale committed artifacts.

## In scope

- Add serde and schemars metadata.
- Emit JSON Schema and annotated `*.example.*` files.
- Generate the `Default` column in specification 03 while specifications 05 and 17 retain policy prose.
- Add a pre-commit freshness hook and focused documentation updates.

## Out of scope

- Writing user configuration as a command side effect.
- A second configuration namespace.
- Hand-maintained duplicate defaults.

## Governed by

- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — owns generated artifact placement.
- [`../../../reference/spec/03-artifact-model.md`](../../../reference/spec/03-artifact-model.md) — owns fields and the default column.
- [`../../../reference/spec/04-composition-and-determinism.md`](../../../reference/spec/04-composition-and-determinism.md) — owns purity and generation boundaries.
- [`../../../explanation/configuration-and-composition.md`](../../../explanation/configuration-and-composition.md) — owns the subsystem design.
- [`../../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../../decisions/ADR-0012-generate-config-examples-from-types.md) — requires type-driven examples.
- [`../../../decisions/ADR-0047-manifest-carries-no-schema-version.md`](../../../decisions/ADR-0047-manifest-carries-no-schema-version.md) — fixes compatibility at the key surface.
- [`../../../decisions/ADR-0057-manifest-grammar-and-validation-boundary.md`](../../../decisions/ADR-0057-manifest-grammar-and-validation-boundary.md) — fixes the closed grammar.

## Acceptance

When type defaults or fields change, the generator SHALL update schema, examples, and default cells from that same source.

If committed artifacts are stale, then check mode SHALL fail without rewriting them.

When a user runs ordinary commands, vivarium SHALL NOT create configuration artifacts.

## Rabbit holes

- The generator becomes a policy owner — escape: keep narrative policy in specifications 05 and 17.
- Examples become scaffolding — escape: keep them copy-only artifacts under N13.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
