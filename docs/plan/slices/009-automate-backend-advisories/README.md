# 009 — Automate backend advisories

## Goal

Automate routine backend currency and advisory evidence while preserving human-triggered release responsibility.

## Appetite

2 implementation sessions.

## Core

Scheduled automation proposes lock moves, CI scans Rust advisories, and the built runner closure is scanned when available; nothing silently updates a user's project.

## In scope

- Add a scheduled workflow for vivarium's `nix/flake.lock` and `Cargo.lock`.
- Enable the `cargo deny` advisory gate.
- Add a gated closure scanner and reviewable output.
- Verify the publishing guide's current owner and backup text.

## Out of scope

- Automatic publication.
- Background user-project pin moves.
- Changes to response windows.
- An invented backup maintainer.

## Governed by

- [`../../../guides/publishing.md`](../../../guides/publishing.md) — owns the release procedure and role assignment.
- [`../../../../SECURITY.md`](../../../../SECURITY.md) — owns advisory response windows.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns pinned backend facts.
- [`../../../decisions/ADR-0049-backend-is-a-closure-member.md`](../../../decisions/ADR-0049-backend-is-a-closure-member.md) — fixes backend ownership.
- [`../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — fixes the two-clock response model.
- [`../../../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md`](../../../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) — fixes the truthful staffing posture.

## Acceptance

When the scheduled workflow runs, it SHALL open reviewable lock changes and SHALL NOT publish a release.

If a Rust advisory applies, then CI SHALL fail with actionable evidence.

When a runner closure exists, the closure scanner SHALL inventory the pinned backend and SHALL report applicable findings.

If no fixed upstream revision exists, then the automation SHALL NOT claim resolution.

## Rabbit holes

- The workflow becomes release authority — escape: make lock changes PR-only.
- Closure scanning before a build exists — escape: use an explicit gate rather than false success.
- Backup remains `none` — escape: preserve truthful documentation instead of inventing staffing.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
