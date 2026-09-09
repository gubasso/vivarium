# 007 — Build the test lanes

## Goal

Implement the six proof lanes and a network fixture that distinguishes name-level from address-level enforcement.

## Appetite

4 implementation sessions.

## Core

Each required lane has at least one ungated self-check and proves the property assigned by ADR-0076 and ADR-0077; purity does not silently depend on an unvalidated experimental format.

## In scope

- Add structured-golden, text-contract-golden, and non-invasion lanes: the helpers exist and the assertions are folded into `local` trials rather than gathered.
- Use the `eval` lane for evaluation coverage.
- Revalidate Q-007 before implementing purity inspection.
- Build an egress fixture with two `.test` names on different addresses inside the VM namespace and only one allowed.

## Out of scope

- Changing the lane taxonomy.
- External `.example` DNS fixtures.
- The nextest profiles and the runtime gate: `ADR-0114` enacted both.
- Tests that scan for absence instead of constructing it.

## Governed by

- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — defines the six lanes and proof model.
- [`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) — defines the discriminating egress fixture.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns pinned Nix/backend facts.
- [`../../../decisions/ADR-0076-test-lanes-and-what-each-proves.md`](../../../decisions/ADR-0076-test-lanes-and-what-each-proves.md) — fixes the taxonomy.
- [`../../../decisions/ADR-0077-proving-absence-in-the-purity-and-non-invasion-lanes.md`](../../../decisions/ADR-0077-proving-absence-in-the-purity-and-non-invasion-lanes.md) — fixes absence proofs.
- [`../../../../.config/nextest.toml`](../../../../.config/nextest.toml) — owns runner profiles.
- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — owns the executable lanes and where each runs.
- [`../../../../tests/support/preflight.rs`](../../../../tests/support/preflight.rs) — owns what each lane needs of its host.

## Acceptance

When the evaluation lane runs on a host with Nix and no KVM, it SHALL execute its tests.

When the purity lane runs, it SHALL prove closure absence using a revalidated representation.

When the egress fixture runs, it SHALL resolve allowed and denied names to different addresses and SHALL exercise both outcomes.

## Rabbit holes

- Experimental derivation JSON — escape: resolve Q-007 and record the revision.
- Two names on one address — escape: arrange the fixture before any boot.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, Q-007 exits through a `Revisions` line, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

- 2026-09-08, before the slice started. [`ADR-0114`](../../../decisions/ADR-0114-a-lane-runs-where-a-place-names-it.md) enacted the executable half of this slice's scope — one nextest profile per lane, selected by binary name — because the runtime gate this slice was going to build on had failed in CI and had to be removed to unblock the release migration. What this slice still owns is the three proof lanes without a home of their own: structured golden, text-contract golden, and non-invasion as a fixture-enforced check rather than an opt-in helper. The appetite is unchanged: the profile work was a fraction of it, and the removed acceptance line about exit `4` no longer describes anything, because nothing is ignored.
