# 010 — Repair the host runbooks

## Goal

Return every host runbook to a state where a `FAIL` means the product is wrong, so the harness can carry evidence again instead of reporting its own breakage.

## Appetite

3 implementation sessions.

## Core

The four failing runbooks either pass or fail for a stated product reason, and [KI-0001](../../../reference/known-issues/KI-0001/README.md)'s recheck can be performed.

## In scope

Ordered, because the first item decides how much of the rest is real work and how much is one host's configuration. Do not reorder without recording why.

1. Resolve Q-011 by diagnosing the failures directly and demonstrating the diagnosis with a repaired-lane run on this host. No second host is available, so the question exits on cause rather than on comparison; see `Revisions`. Record the run in the [verification harness](../../../reference/microvm-verification-harness.md) either way.
2. Fix the reporting cluster: `store-gc-interlock-check`, `store-pressure-check`, and `share-benchmark-check` boot a guest whose console capture stops seconds into a leg budgeted for tens of minutes, and then read a guest that "exited `0`" without its diagnostic marker. This blocks the most — three lanes and KI-0001 — so it precedes the narrower faults. Note that `first-microvm-check`'s measurement guest and `guest-agent-check`'s agent guest both do report, so the difference between how those scripts drive a launch and how these do is the first place to look.
3. Fix the retained runtime directory. All four lanes end with entries still in the runtime directory against an allowlisted cleanup that is supposed to empty it, so this is one defect with four witnesses rather than four defects.
4. Fix `share-benchmark-check`'s pool sweep, where pools 1, 2, and 4 fail to start and pool 0 starts. Only after item 2, because a guest that cannot report and a guest that cannot start may be the same fault seen twice.
5. Settle `first-microvm-check`'s posture cluster, four independent checks: a required transient-unit property the manager does not report; two processes observed per virtiofsd role where one is expected, the second holding a full capability set; a full bounding set on cloud-hypervisor; and no `--landlock` in the cloud-hypervisor argv on a host that reports Landlock. Each MUST be settled as a question before it is settled as a fix. The launch spec does carry `landlock_enable`, so the check may be reading argv for a fact that now lives in the API JSON; and [ADR-0099](../../../decisions/ADR-0099-the-bounding-set-drop-is-best-effort.md) already decides that the bounding-set drop is best-effort, which a check demanding an empty one would contradict.
6. Perform KI-0001's recheck, `tests/host/store-pressure-check --arm e`, once item 2 makes it possible, and move that entry on the result.

## Out of scope

- Changing what any runbook measures, or its thresholds.
- `guest-agent-check` and `store-density-check`, which pass.
- The measurement images' content, except where item 2 finds the fault there.
- Automating any of these lanes into CI, which is [slice 007](../007-build-test-lanes/README.md)'s.

## Governed by

- [`../../open-questions.md`](../../open-questions.md) — owned Q-011, whose exit gated this slice's shape; the question has since left through the harness entry this slice wrote.
- [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md) — owns what each runbook proves and the findings register this slice writes to.
- [`../../../reference/known-issues/KI-0001/README.md`](../../../reference/known-issues/KI-0001/README.md) — owns the recheck this slice unblocks.
- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — owns the lane taxonomy these scripts sit beside.
- [`../../../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md`](../../../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md) — fixes the declared descriptor budget the posture checks read.
- [`../../../decisions/ADR-0096-the-share-worker-pool-takes-the-daemon-default.md`](../../../decisions/ADR-0096-the-share-worker-pool-takes-the-daemon-default.md) — fixes the worker pool the sweep varies.

## Acceptance

When the four failing runbooks are re-run after repair, the harness SHALL record the cause each failure is attributed to and which failures the repair cleared, and Q-011 SHALL exit through that record.

While a guest diagnostic is expected, `store-gc-interlock-check`, `store-pressure-check`, and `share-benchmark-check` SHALL each observe the marker they read, or SHALL report a stated product reason for its absence.

When any of the four runbooks completes, the runtime directory SHALL contain no entries the allowlisted cleanup is required to remove.

While `share-benchmark-check` sweeps the pool, every declared pool size SHALL start a guest.

If a `first-microvm-check` posture check fails, then either the product SHALL be corrected or the check SHALL be corrected with its reason recorded, and no such check SHALL remain failing without one.

When KI-0001's recheck runs to a verdict, that entry SHALL move to the status the verdict supports.

## Rabbit holes

- Treating one host as the target host — escape: item 1 runs first and nothing below it is scoped until it answers. With the answer now coming from cause rather than from a second machine, every finding states the mechanism it rests on, so a reader can judge it without the comparison.
- Chasing four failures as four bugs — escape: the retained runtime directory and the missing marker each span multiple lanes; fix the span, not the witness.
- Fixing a check to make it pass — escape: item 5 requires the argv-versus-API question to be answered before either side is touched.
- Refreshing harness figures from a repaired lane and treating them as unchanged — escape: a lane that could not report never confirmed its old figures, so a repaired run is a new measurement with a new date.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, Q-011 has exited through the harness entry that records its cause, KI-0001 carries a verdict at its pinned version, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

2026-08-10 — item 1 no longer runs the five runbooks on a second capable host, and the first `Acceptance` assertion moves with it. No second host is available and none is planned, so the step could not be performed and every item it gates would have stayed unscoped. Reading the four lanes supplied the separation it existed to provide: `first-microvm-check` was taught the supervisor handoff that [`ADR-0097`](../../../decisions/ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md) decides and the other three were not, which is a defect in this repository rather than a property of one machine. The ordering is otherwise unchanged, and the limit the second host would have covered is stated in the harness instead of assumed away.

2026-08-10 — item 2 and item 5 restate their symptoms. Both were written from a compressed summary that the evidence contradicts: the failing consoles are not zero-byte but truncated seconds in, the failing unit property is not `IOWeight`, and cloud-hypervisor's bounding set is a fourth check rather than part of the virtiofsd one. Goal, Core, Appetite and the remaining `Acceptance` assertions are unchanged.
