# 004 — Enforce the egress allowlist

## Goal

Enforce the specified name-level egress allowlist before a guest can use an answer while preserving open egress by default.

## Appetite

4 implementation sessions.

## Core

Each VM has isolated networking and a gating resolver that installs address policy before releasing permitted DNS answers; denials reject quickly.

## In scope

- Resolve Q-005 and select the networking crate seams.
- Implement network namespace, tap, nftables, and resolver libraries.
- Implement address expiry and update behavior from the specification.
- Run Q-006's direct vhost-user experiment and use the tap-plus-uplink fallback if it fails.
- Add implementation-level ordering and denial tests.
- Land `workflow_05_restrict_egress_allowlist` unskipped on a capable host and delete the last pre-push subtraction, returning `profile.pre-push` to `kind(test)`. This trial is the final one the acceptance harness holds against an unimplemented verb, so this slice is where the stage becomes a real gate again; [`../../sequencing.md`](../../sequencing.md) owns the order and the two slices that narrowed the clause before it.

## Out of scope

- Changing allowlist syntax or its default.
- Wildcard support.
- A system DNS proxy.
- Inbound networking.

## Governed by

- [`../../../reference/spec/05-networking-and-egress.md`](../../../reference/spec/05-networking-and-egress.md) — defines the enforcement contract.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — defines N8.
- [`../../../explanation/networking-and-egress.md`](../../../explanation/networking-and-egress.md) — owns the subsystem topology.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns backend network modes.
- [`../../../decisions/ADR-0007-default-open-egress.md`](../../../decisions/ADR-0007-default-open-egress.md) — fixes the default.
- [`../../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md`](../../../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md) — fixes host enforcement and denial behavior.
- [`../../../decisions/ADR-0064-egress-allowlist-enforcement-model.md`](../../../decisions/ADR-0064-egress-allowlist-enforcement-model.md) — fixes the gating-resolver model.

## Acceptance

While egress is open, the sandbox SHALL reach destinations without allowlist enforcement.

While allowlist mode is active, when a permitted name resolves, the gating resolver SHALL install its addresses before releasing the answer.

If a denied name or address is used, then the host filter SHALL reject the connection inside the specified budget.

When the uplink topology experiment fails, the tap-plus-uplink fallback SHALL preserve the same contract.

## Rabbit holes

- The direct vhost-user combination is undocumented — escape: run one bounded experiment, then use the fallback.
- Resolving before filtering — escape: make answer release depend on successful rule installation.
- Crate search sprawl — escape: evaluate Q-005 against the four explicit seams only.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, Q-005 and Q-006 exit through their recorded slice revision and experiment, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
