<!-- markdownlint-configure-file {"MD043": {"headings": ["?", "## Goal", "## Appetite", "## Core", "## In scope", "## Out of scope", "## Governed by", "## Acceptance", "## Rabbit holes", "## Done when", "## Revisions"], "match_case": true}} -->

# 008 — Enforce implementation status

## Goal

Make every current-state claim in `implementation-status.md` mechanically consistent with parser commands, test existence, and runtime gating.

## Appetite

2 implementation sessions.

## Core

The three promised checks fail on missing trials, false `Implemented` rows, and parser commands absent from the table.

## In scope

- Extract the status table and parser command inventory.
- Resolve test names and classify ignore and runtime gates.
- Provide a deterministic check command, pre-commit and CI wiring, and self-tests.

## Out of scope

- Changing command semantics or statuses merely to satisfy the check.
- General documentation link checking.

## Governed by

- [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) — owns current implementation truth and the three assertions.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — owns parser-visible commands.
- [`../../../explanation/cli-and-diagnostics.md`](../../../explanation/cli-and-diagnostics.md) — owns the current-state boundary.
- [`../../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../../decisions/ADR-0012-generate-config-examples-from-types.md) — provides the generated-check precedent.
- [`../../../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md`](../../../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md) — makes status load-bearing.
- [`../../../../tests/user_workflows.rs`](../../../../tests/user_workflows.rs) — owns acceptance trial declarations.

## Acceptance

If a Trial cell names no test, then the status check SHALL fail.

If an `Implemented` row names a skipped or ignored trial, then the status check SHALL fail.

If the parser exposes a subcommand with no status row, then the status check SHALL fail.

When the page and the code agree, the status check SHALL exit successfully without rewriting the page.

## Rabbit holes

- Textual parsing is too permissive — escape: define a deliberately narrow table and parser schema.
- Designed trials are treated as proof — escape: inspect runtime gates and ignore markers.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
