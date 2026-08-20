# 025 — The fleet is visible

## Goal

Several sandboxes run at once and nothing shows the fleet. There is no per-sandbox memory figure, no count of the sessions attached to one, and no host reading beside them, so a user running one sandbox per project arbitrates by hand from `free -h` and from memory of what they started. After this slice `viv status -g` names every sandbox with its declared ceiling beside its measured use, and `viv status` says how many sessions are attached.

## Appetite

3 implementation sessions.

## Core

`viv status -g` enumerates every sandbox, in both the human and JSON renderings, showing each one's declared ceiling beside its measured use and the host's own reading beside the fleet; `viv status` reports the number of live sessions. The one outcome the core forbids is a row that reports a declaration where a reader will read a measurement.

## In scope

Ordered, because what the number means has to be settled before anything renders one.

1. Settle what `mem_used_bytes` measures, and label it where it is reported. [`ADR-0082`](../../../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) rejects hypervisor resident-set size for this purpose: guest RAM is backed by a file object mapped shared, so resident size falls the instant a range is advised away, whether or not a page was freed. It names allocated blocks of the backing object as the metric and proportional set size as the cross-check. [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) meanwhile defines `runtime.mem_used_bytes` as the resource scope's own memory. Reconcile the two before item 2 reads anything: either the scope reading satisfies that record and this slice says why, or the reported figure is the backing-object allocation and the command surface is amended to match. Either way the metric is named where it is shown, which is that record's own consequence.
2. Read the fleet. Per-sandbox memory comes from the transient user service [`../../../../src/launch/systemd.rs`](../../../../src/launch/systemd.rs) already starts with `MemoryAccounting=yes`, under one `vivarium.slice`. Host readings reuse the `MemAvailable` parse [`../../../../src/doctor/host.rs`](../../../../src/doctor/host.rs) already carries for `host-memory-headroom`, so there is one reader rather than two. Where the memory controller is not delegated to the user's own manager, `mem_used` reports as unavailable rather than as zero, which is the degradation [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) requires and which `host-cgroup2-delegation` already detects.
3. Enumerate sandboxes rather than bindings. Under [`ADR-0107`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) the registry is a derived index from declared workspaces back to the manifest that owns them, so enumeration reads that index or rebuilds it from the manifests. Registry I/O answers `74` and a malformed index `78`, which [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) already assigns.
4. Count the sessions. [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) makes sessions counted and not tracked — the number of live connections on the control socket, nothing more — and `viv status` is where the count appears. Slice 013 cut this and named it as owed.
5. Rewrite the two output contracts for the keyed-on-manifest model, then render them. The `-g` record in [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) carries one `project_path` per row and the table in [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) heads its first column with a project; under [`ADR-0108`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) a sandbox owns a set of workspaces and is named by its manifest, which that record already uses as the identity field. Both renderings live in [`../../../../src/cli/render.rs`](../../../../src/cli/render.rs). The `host` object is present even when nothing is running, because it is what lets one command answer whether the host is overcommitted.
6. Report a workspace whose directory is gone. `path_missing` survives the rekey with a narrower meaning — a declared workspace directory that no longer exists — and is reported and never removed, because enumerating is read-only. What does not survive is the binding warning and its `viv unbind` remedy: [`ADR-0054`](../../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md) is superseded and the verb goes with the binding it removed.
7. Consume [`Q-015`](../../open-questions.md), which this slice's own rendering is the first consumer of: either the command surface admits `null` for a running sandbox whose launch record cannot be read, or the shipped floor is the reported ceiling and the guarantee stands.

## Out of scope

- Acting on any reading. Refusing, warning, sweeping, and reclaiming belong to the three slices after this one; this slice reports and nothing else.
- The orphaned sandbox whose manifest was renamed or removed. That is [slice 021](../021-the-manifest-is-the-sandbox/README.md) item 6, and it is that slice's ordered remainder — if it is cut there, this slice inherits it and says so under `Revisions` rather than shipping two reports of the same condition.
- Disk measurement beyond what `viv volume list` already reads. The allocated-against-virtual pair is joined from that command's own reading, not measured a second way.
- Ordered remainder, cut first when the appetite binds: the uptime column, the pressure-stall reading beside the host memory figure, and the near-ceiling annotation on a row.

## Governed by

- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — fixes the `-g` record and the `resources`/`runtime` split, and is the page item 5 rewrites.
- [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) — owns the reporting table and the ceiling-beside-reality contrast this slice exists to show.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — makes sessions counted rather than tracked, which is what item 4 implements.
- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — owns the state root and the index item 3 enumerates.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — assigns the codes an unreadable or malformed index answers with.
- [`../../../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md`](../../../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) — fixes what a memory figure may be measured on, which item 1 reconciles with the reported field.
- [`../../../decisions/ADR-0035-elastic-guest-memory-model.md`](../../../decisions/ADR-0035-elastic-guest-memory-model.md) — fixes the elasticity the measured column reports on.
- [`../../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md`](../../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) — fixes the per-sandbox scope item 2 reads, whose launch half slice 002 already realized.
- [`../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) — fixes the key every row carries and the inversion item 3 enumerates.
- [`../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) — fixes why a row carries a set of workspaces rather than one path.
- [`../../open-questions.md`](../../open-questions.md) — carries `Q-015`, which item 7 consumes.

## Acceptance

When two sandboxes from different manifests are running, `viv status -g` SHALL name both in the human and the JSON rendering, and at least one row SHALL show a measured use that differs from its declared ceiling, so the two columns are demonstrated to be different readings rather than one value printed twice.

When a sandbox declares more than one workspace, it SHALL appear as one row naming every declared workspace, and no sandbox SHALL appear more than once.

When a declared workspace's directory has been removed, its sandbox SHALL still be enumerated, the row SHALL carry `path_missing`, and the directory SHALL be named on stderr so the JSON rendering stays clean. Nothing SHALL be removed from the index by the enumeration.

When sessions are attached to a running sandbox, `viv status` SHALL report their number, and the number SHALL change with an attach and a detach.

If no sandbox is running, then `viv status -g` SHALL emit an empty list beside a populated `host` object and SHALL exit `0`. If the derived index cannot be read it SHALL exit `74`, and if it is malformed it SHALL exit `78`.

Wherever a memory figure is reported, the rendering SHALL name what was measured, and the figure SHALL come from the single reader item 2 establishes.

## Rabbit holes

- Building a monitor because a table wants fresh numbers — escape: `viv status -g` is one read at one moment; nothing samples, polls, or runs between invocations, which is the charter's own no-go on background arbitration.
- Reporting resident-set size because it is the reachable number — escape: `ADR-0082` names exactly that mistake and the reason it reports success that did not happen; item 1 is the reconciliation and it comes first for this reason.
- Growing a second measurement path for the fleet because the per-sandbox one reads differently — escape: one reader, named in item 2, and the three slices after this one consume it rather than adding their own.
- Reviving the binding to explain a row — escape: the sandbox is keyed on its manifest and the index is derived; a row that needs a binding to make sense is a row written against machinery slice 021 deleted.
- Reaping an entry whose directory is gone because the row looks like garbage — escape: enumerating is read-only, an absent directory may be an unmounted filesystem, and reporting it is the whole of this slice's job.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, `Q-015` is closed and removed with its chosen exit recorded where that exit belongs, item 1's reconciliation is written where the metric is defined rather than only in this document, the rows this slice changes are moved in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-20, before any work started, from the gap paragraph in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) that groups the surfaces sitting inside verbs already marked implemented. `viv status -g` refuses by name in [`../../../../src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs), whose own comment records that it had no trial in the slice that wrote it and was the first thing that slice's appetite would cut. This slice is where it stops being cut.

Sequenced behind [slice 021](../021-the-manifest-is-the-sandbox/README.md) rather than ahead of it, for the reason [`../../sequencing.md`](../../sequencing.md) already records for five other slices: everything here reads per-sandbox state and runtime paths and renders one row per sandbox, which is precisely what `ADR-0107` rekeys. Written first, it would be rewritten line for line, and its trials with it.

Sequenced ahead of slices 005 through 009, 018, and 024 for the reason that page records for slice 019: the surface is specified, referenced by [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) and by the guides, and consumed by nothing. A documented fleet table with nothing behind it is the same defect as a declaration surface nothing reads.

Item 1 is first rather than last because of a conflict found while shaping. `ADR-0082` was written for the elasticity verification and rejects the obvious memory metric; the command surface fixes a field that reads like the obvious metric. Nothing had reconciled them because nothing had reported the number yet. Rendering first and reconciling afterwards would ship a figure whose meaning is decided by whichever reader was easiest, which is the case that record exists to prevent.
