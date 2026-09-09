# 008 — Enforce implementation status

## Goal

Make every current-state claim in `implementation-status.md` mechanically consistent with parser commands, test existence, and runtime gating.

## Appetite

2 implementation sessions.

## Core

The three promised checks fail on missing trials, false `Implemented` rows, and parser commands absent from the table.

## In scope

- Extract the status table and parser command inventory.
- Resolve test names across the acceptance binaries and classify ignore markers and lane membership.
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
- [`../../../../tests/local_workflows.rs`](../../../../tests/local_workflows.rs), [`../../../../tests/eval_workflows.rs`](../../../../tests/eval_workflows.rs), and [`../../../../tests/boot_workflows.rs`](../../../../tests/boot_workflows.rs) — own the acceptance trial declarations.
- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — owns which lane runs a given trial, and where.

## Acceptance

If a Trial cell names no test, then the status check SHALL fail.

If an `Implemented` row names an ignored trial, or one in a lane no place runs, then the status check SHALL fail.

If the parser exposes a subcommand with no status row, then the status check SHALL fail.

When the page and the code agree, the status check SHALL exit successfully without rewriting the page.

## Rabbit holes

- Textual parsing is too permissive — escape: define a deliberately narrow table and parser schema.
- Designed trials are treated as proof — escape: inspect ignore markers and the lane register that says where each binary runs.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

2026-09-08 — `ADR-0114` split the single acceptance binary into lane-named ones and deleted the runtime gate, so the `Governed by` list now names all three and the register that places them, and the checks classify lane membership rather than a gate a trial applied to itself. No change to Goal, Core, Appetite, or Acceptance beyond restating the second assertion in the new vocabulary.
