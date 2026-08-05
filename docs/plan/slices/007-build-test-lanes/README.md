<!-- markdownlint-configure-file {"MD043": {"headings": ["?", "## Goal", "## Appetite", "## Core", "## In scope", "## Out of scope", "## Governed by", "## Acceptance", "## Rabbit holes", "## Done when", "## Revisions"], "match_case": true}} -->

# 007 — Build the test lanes

## Goal

Implement the six proof lanes and a network fixture that distinguishes name-level from address-level enforcement.

## Appetite

4 implementation sessions.

## Core

Each required lane has at least one ungated self-check and proves the property assigned by ADR-0076 and ADR-0077; purity does not silently depend on an unvalidated experimental format.

## In scope

- Add unit, structured-golden, text-contract-golden, evaluation, purity, and non-invasion lanes and nextest profiles.
- Use `GateLevel::ConfigEval` for evaluation coverage.
- Revalidate Q-007 before implementing purity inspection.
- Build an egress fixture with two `.test` names on different addresses inside the VM namespace and only one allowed.

## Out of scope

- Changing the lane taxonomy.
- External `.example` DNS fixtures.
- All-ignored profiles.
- Tests that scan for absence instead of constructing it.

## Governed by

- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — defines the six lanes and proof model.
- [`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) — defines the discriminating egress fixture.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns pinned Nix/backend facts.
- [`../../../decisions/ADR-0076-test-lanes-and-what-each-proves.md`](../../../decisions/ADR-0076-test-lanes-and-what-each-proves.md) — fixes the taxonomy.
- [`../../../decisions/ADR-0077-proving-absence-in-the-purity-and-non-invasion-lanes.md`](../../../decisions/ADR-0077-proving-absence-in-the-purity-and-non-invasion-lanes.md) — fixes absence proofs.
- [`../../../../.config/nextest.toml`](../../../../.config/nextest.toml) — owns runner profiles.
- [`../../../../tests/support/mod.rs`](../../../../tests/support/mod.rs) — owns runtime gate levels.

## Acceptance

When a nextest profile runs without its external gate, the profile SHALL execute at least one self-check and SHALL NOT exit 4.

When `ConfigEval` is available without KVM, the evaluation lane SHALL execute its tests.

When the purity lane runs, it SHALL prove closure absence using a revalidated representation.

When the egress fixture runs, it SHALL resolve allowed and denied names to different addresses and SHALL exercise both outcomes.

## Rabbit holes

- Experimental derivation JSON — escape: resolve Q-007 and record the revision.
- An all-ignored lane — escape: use the `harness_self_check` pattern.
- Two names on one address — escape: gate fixture construction before boot.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, Q-007 exits through a `Revisions` line, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
