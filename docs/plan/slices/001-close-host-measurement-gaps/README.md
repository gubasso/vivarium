<!-- markdownlint-configure-file {"MD043": {"headings": ["?", "## Goal", "## Appetite", "## Core", "## In scope", "## Out of scope", "## Governed by", "## Acceptance", "## Rabbit holes", "## Done when", "## Revisions"], "match_case": true}} -->

# 001 — Close host measurement gaps

## Goal

Replace the last ambiguous host findings with measurements that can retain or revise the governing decisions.

## Appetite

3 implementation sessions.

## Core

Instrument a real collection so per-path decisions and actual upper-filesystem reclamation are observable; do not guarantee a particular cause or policy outcome.

## In scope

- Add collector instrumentation and run one discriminating real-host arm.
- Run a concurrent pool-size arm over `{0,1,2,4}` with repeated observations.
- Recover guest userspace timing or establish its unavailability and choose a replacement metric.
- Update the harness, backend reference, and any decision changed by evidence.

## Out of scope

- Another uninstrumented collection boot.
- A second collection mechanism.
- Disabling the worker pool from single-process results.
- Generalized claims about host filesystems.

## Governed by

- [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md) — owns prior measurements and method.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns pinned upstream facts used by the arms.
- [`../../../explanation/guest-store-and-volumes.md`](../../../explanation/guest-store-and-volumes.md) — owns the storage topology and collection boundary.
- [`../../../explanation/shared-filesystems.md`](../../../explanation/shared-filesystems.md) — owns the worker-pool boundary.
- [`../../../decisions/ADR-0051-share-worker-pool-small-non-zero-uniform.md`](../../../decisions/ADR-0051-share-worker-pool-small-non-zero-uniform.md) — fixed the pool policy this slice measured, and was superseded by its evidence.
- [`../../../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md`](../../../decisions/ADR-0089-the-guest-store-is-collected-on-space-pressure.md) — fixes the collection policy.
- [`../../../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md`](../../../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md) — fixes the measurement-image seam.

## Acceptance

When collection crosses `min-free`, the measurement harness SHALL record requested and reported bytes, each attempted path's classification, guest `statvfs`, and host allocated-block deltas.

When a concurrent workload runs, the measurement harness SHALL record enough repetitions to retain or revise ADR-0051 without extrapolating from a single process.

When boot timing runs, the measurement harness SHALL record guest userspace time, or a source-grounded unavailable result together with a named replacement metric.

## Rabbit holes

- Treating the lower-only path count as a cause — escape: label it as a hypothesis until per-path evidence exists.
- Noisy benchmarks — escape: use a fixed tree and closure, dropped caches, repetitions, and a local-volume control.
- Unavailable timing — escape: read the owning source and select a replacement metric instead of parsing empty output as zero.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, the three questions this slice carried exit through recorded measurements, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

2026-08-05 — `In scope` item 3 exits through its acceptance clause's second branch. `systemd-analyze time` is refused, not empty: it requires `FinishTimestampMonotonic`, which PID 1 sets only when the boot transaction's job queue empties, and every measurement leg plus the stop unit is a job in that transaction. Recorded as unavailable with `guest_userspace_to_probe_ms` named in its place. No change to Goal, Core, Appetite or Acceptance.

2026-08-05 — `In scope` item 2 spent one extra lane run. The first concurrent workload discovered its own file list, so `readdirplus` answered it and it never reached the daemon; the numbers were discarded and the leg rebuilt around a list built before the timed region. Within appetite.
