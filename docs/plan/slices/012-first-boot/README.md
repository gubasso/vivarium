# 012 — First boot

## Goal

Boot the derivation [slice 011](../011-resolve-and-evaluate/README.md) produces, so that `viv start`, `viv status`, and `viv stop` become real commands rather than designed ones.

## Appetite

3 implementation sessions.

## Core

A detached `viv start` boots the built guest, `viv status` reports it running, and `viv stop` ends it cleanly, leaving the runtime directory with no entry the allowlisted cleanup is required to remove.

## In scope

Ordered, because the first item decides whether the rest is wiring or repair. Do not reorder without recording why.

1. Hand slice 011's build output to the existing launch construction in [`../../../../src/launch/spec.rs`](../../../../src/launch/spec.rs), and boot it once by hand. This is reuse: the supervisor, policy constructor, and transient-unit renderer already exist and are host-proven for the diagnostic runner.
2. Confirm or refute that [slice 010](../010-repair-the-host-runbooks/README.md)'s repairs hold for a manifest-built guest. The named risks are its findings, not new guesses: a retained runtime directory, truncated console capture, and pool sizes that fail to start. A defect that reappears here is a product defect, not a harness one.
3. Implement `viv start` detached, including the transient-unit handoff that owns the VM lifetime.
4. Implement `viv status` against the state model.
5. Implement `viv stop` and its teardown boundary, and assert the runtime directory is empty afterwards.
6. Resolve Q-008, which asks whether `start` returns `74`. It could not be answered while `start` was designed only; it can be answered here.
7. Land the two `Virtualization` trials this slice's `Acceptance` names, unskipped.

## Out of scope

- Networking configuration of any kind. Egress is open by default, so a first boot needs no network work; [slice 004](../004-enforce-egress-allowlist/README.md) owns enforcement.
- Reaching into the running guest. [Slice 013](../013-exec-and-shell/README.md) owns `viv exec` and `viv shell`.
- Volume persistence across a stop and start, which is [slice 014](../014-workspace-and-persistence/README.md)'s.
- New launch, confinement, or supervision code. If this slice writes any, that is a finding worth recording, not a scope expansion.
- Generations, `viv update`, `viv gc`, and store reclamation under pressure.

## Governed by

- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — defines the lifecycle verbs, runtime lifetime, and cleanup.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — defines the `start` row Q-008 disputes.
- [`../../../reference/spec/16-logging-and-diagnostics.md`](../../../reference/spec/16-logging-and-diagnostics.md) — defines the console capture item 2 checks.
- [`../../../explanation/launch-and-supervision.md`](../../../explanation/launch-and-supervision.md) — owns the launch topology this slice reuses.
- [`../../../explanation/state-and-lifecycle.md`](../../../explanation/state-and-lifecycle.md) — owns the state model `viv status` reads.
- [`../../../explanation/guest-store-and-volumes.md`](../../../explanation/guest-store-and-volumes.md) — owns how the guest store reaches the guest.
- [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md) — owns the runbook findings item 2 rechecks.
- [`../../../decisions/ADR-0007-default-open-egress.md`](../../../decisions/ADR-0007-default-open-egress.md) — fixes the open default that keeps networking out of scope.
- [`../../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../../decisions/ADR-0009-launch-time-workspace-path-injection.md) — fixes how the workspace path reaches launch.
- [`../../../decisions/ADR-0013-vm-lifecycle-and-up.md`](../../../decisions/ADR-0013-vm-lifecycle-and-up.md) — fixes the lifecycle model.
- [`../../../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md`](../../../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md) — fixes the mount paths launch constructs.
- [`../../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](../../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) — fixes the teardown boundary `viv stop` stops at.
- [`../../../decisions/ADR-0030-vm-status-and-state-model.md`](../../../decisions/ADR-0030-vm-status-and-state-model.md) — fixes what `viv status` reports.
- [`../../../decisions/ADR-0038-guest-store-sharing.md`](../../../decisions/ADR-0038-guest-store-sharing.md) — fixes how the host store reaches the guest.
- [`../../../decisions/ADR-0055-runtime-directory-is-required.md`](../../../decisions/ADR-0055-runtime-directory-is-required.md) — fixes the runtime directory as a precondition.
- [`../../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md`](../../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md) — fixes the outer lifetime bound.
- [`../../../decisions/ADR-0067-volume-prune-and-first-boot-home.md`](../../../decisions/ADR-0067-volume-prune-and-first-boot-home.md) — fixes the home a first boot finds.
- [`../../../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md`](../../../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md) — fixes inner-store provisioning.
- [`../../../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md`](../../../decisions/ADR-0087-the-inner-store-persists-on-its-own-volume.md) — fixes where the inner store lives.
- [`../../../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md`](../../../decisions/ADR-0088-the-guest-store-is-a-local-overlay-store.md) — fixes the overlay shape.
- [`../../../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md`](../../../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md) — fixes the handoff item 3 implements.

## Acceptance

When a bound project is started, `viv start` SHALL boot the guest system derivation slice 011 built and `workflow_01_first_time_bind_boot` SHALL pass unskipped.

When two projects resolve to the same identity, the second SHALL take the specified suffix and `workflow_02_identity_collision_suffix` SHALL pass unskipped.

When `viv stop` completes, the runtime directory SHALL contain no entry the allowlisted cleanup is required to remove.

While a detached `viv start` runs, the console capture SHALL span from before the first guest write until VM exit, and SHALL NOT truncate early.

If `start` fails on its runtime directory or its socket, then the code it returns SHALL be the one [`14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) admits, with any discrepancy resolved rather than carried.

## Rabbit holes

- Treating a slice 010 finding that reappears as a harness problem — escape: the harness lanes pass now, so a defect seen here is in the product path this slice exercises.
- Rewriting launch construction because the manifest-built guest differs from the diagnostic one — escape: the difference is the input, not the launcher; change the spec this slice hands over before changing the code that consumes it.
- Answering Q-008 by editing whichever side is easier — escape: its exit is either a `14-exit-codes.md` amendment or a `Revisions` line here, and one of the two must be written.
- Adding `viv exec` because a booted guest invites it — escape: boot, observe, and stop is the whole core; slice 013 is two sessions away.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial or runbook it names, Q-008 has exited through the amendment or the `Revisions` line it names, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved to Implemented, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
