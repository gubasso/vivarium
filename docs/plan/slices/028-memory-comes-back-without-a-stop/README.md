# 028 — Memory comes back without a stop

## Goal

A guest returns the memory it has finished with, continuously and without being asked, except for the one category it cannot: page cache is not free memory, so reporting never returns it and only the guest can decide to drop it. A long session that has run builds and repository-wide searches therefore climbs toward its ceiling and stays there, and the only reclaim available is stopping the sandbox and paying for a cold start. After this slice a user reclaims that memory on demand, is told what the host got back, and has the same verb for a volume's disk.

## Appetite

4 implementation sessions.

## Core

`viv memory trim` reclaims memory from a running sandbox and reports what the host got back, bounded and synchronous, restoring the guest's headroom immediately; `viv volume trim` does the same for a volume's allocated bytes. The one outcome the core forbids is a reported figure the command cannot substantiate.

## In scope

Ordered, because what the command reports is what makes it worth running.

1. Reach the balloon. The device is already configured for this — it starts at zero, reports free pages, and deflates on out-of-memory pressure ([`../../../../nix/launch-arguments.nix`](../../../../nix/launch-arguments.nix)) — so the work is a control path from the command to a running guest, not a device to add. The operation asks the guest down to a target and then immediately restores its headroom, which is what makes it safe against a busy guest: the guest drops its own caches first and can take memory back the moment it needs it.
2. Derive the target. With no `--to`, the target is the sandbox's measured working set plus headroom — enough to drop cache, not enough to disturb running work — and the reported target is never absent for a run that completed. With `--to`, the operator's figure wins.
3. Settle how far [`ADR-0082`](../../../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) binds the record. That record rejects a pair of readings, because reclamation is asynchronous and a pair cannot see a transition it did not bracket, and calls for a series aligned to announced phase transitions. [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) fixes a before-and-after pair, and this command is bounded and synchronous — it is the bracket that record says a pair lacks. Either the pair is sound for an operation that brackets itself and this slice records why, leaving the series requirement to the elasticity verification it was written for, or the record grows what a pair cannot carry. Whichever holds, the reclaimed figure is floored at zero so a guest that grew reports nothing rather than a negative, a trim that reclaims nothing is a success, and the readings are the same ones [slice 025](../025-the-fleet-is-visible/README.md) reports so the two commands join.
4. Land the disk counterpart. `viv volume trim [<name>]` returns space freed inside a volume to the host image, carrying the same before-and-after shape with one row per volume and a total that needs no arithmetic from the caller, joined to what `viv volume list` already reports. Volumes are trimmed periodically inside the guest anyway, so this command is for impatience rather than for correctness, and it says so where a user reads it.
5. Suggest it. `viv status` names `viv memory trim` when a sandbox's measured use approaches its ceiling while host memory is low — on stderr, as a suggestion, never as an action. This is also the line [slice 026](../026-a-start-checks-the-room/README.md) deliberately withheld from its warning, and landing it here is what completes that warning's cost-ordered list.
6. Fan out. `viv trim` runs both rungs in one invocation, memory first, taking neither command's flags. Its record nests one subtree per resource with no grand total, and a run that ends in failure emits no record at all — the rule `viv stop --all` already carries, inherited rather than invented ([`../../../decisions/ADR-0113-a-reclaim-verb-sits-under-its-resource.md`](../../../decisions/ADR-0113-a-reclaim-verb-sits-under-its-resource.md)). It comes last because it is convenience over two commands that already work, which is also why it is the first thing cut.

## Out of scope

- Trimming on a schedule, on a timer, or on pressure. N23 and the charter's refusal of background arbitration both forbid it, and the whole point of this command is that the person who knows decides to pay its cost.
- Raising a volume's ceiling. What a larger declaration does to an image that already exists is [`Q-020`](../../open-questions.md)'s subject and is a growth operation with its own program set and its own failure code.
- Deduplicating memory between guests. [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) records that this is impossible rather than deferred, because the shared mapping guest memory needs is exactly what same-page merging cannot work on. The second copy is what this command reclaims, which is why the two facts are stated together.
- Trimming every sandbox at once. Reclaiming across the fleet is an arbitration decision wearing a convenience flag.
- Ordered remainder, cut first when the appetite binds: item 6's fan-out, which is convenience over two commands that already work; then validating `--to` against the guest's own floor rather than letting the guest refuse it; then item 5's suggestion.

## Governed by

- [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) — fixes what the reclaim does, why page cache needs it, and the suggestion item 5 lands.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — fixes both records, and is the page item 3 may amend.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — assigns the codes a stopped sandbox, an unreachable agent, and a failed read answer with.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N22 and N23, the ceiling this command works under and the rule that keeps it user-invoked.
- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — owns the volumes item 4 trims.
- [`../../../decisions/ADR-0113-a-reclaim-verb-sits-under-its-resource.md`](../../../decisions/ADR-0113-a-reclaim-verb-sits-under-its-resource.md) — fixes the command names, which flag sits on which, the fan-out's nested record, and the partial-failure rule it inherits.
- [`../../../decisions/ADR-0035-elastic-guest-memory-model.md`](../../../decisions/ADR-0035-elastic-guest-memory-model.md) — fixes the elastic model this command is the user-invoked escalation on top of.
- [`../../../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md`](../../../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) — fixes what a return may be measured on, and what a pair of readings cannot see.
- [`../../../decisions/ADR-0094-guest-memory-posture-takes-the-distribution-defaults.md`](../../../decisions/ADR-0094-guest-memory-posture-takes-the-distribution-defaults.md) — fixes the guest-side posture the reclaim operates against.
- [`../../open-questions.md`](../../open-questions.md) — carries `Q-020`, which this slice names as out of scope rather than consuming.

## Acceptance

When a running guest's own memory has been dirtied — not a file on a volume, which measures host page cache instead — `viv memory trim` SHALL report a reclaimed figure greater than zero, and the host SHALL show the corresponding fall in that sandbox's measured use, read through the same path `viv status` uses so the two commands agree.

When `viv memory trim` runs against a guest that is doing work, that work SHALL still be running afterwards, and the guest SHALL be able to take memory back immediately.

If the sandbox is not running, then `viv memory trim` SHALL exit `75`. If it is running and the agent or backend cannot be reached, then it SHALL exit `69`.

When a trim reclaims nothing, it SHALL exit `0`, because that is a fact about the guest rather than a failure.

When `viv volume trim` runs, each volume SHALL report its allocated size before and after, the total SHALL be their sum, and a sandbox whose volumes have never been materialized SHALL report an empty list and exit `0`.

When `viv trim --json` completes both rungs, the record SHALL carry a `memory` subtree and a `disk` subtree and no top-level `reclaimed_bytes`, and each subtree SHALL be the record its own command emits.

When one rung of `viv trim` fails, the other rung SHALL still run, each failure SHALL be named on stderr, the exit code SHALL be the first failure's category, and no record SHALL appear on stdout.

Wherever a reclaimed figure is reported, the rendering SHALL name what was measured.

## Rabbit holes

- Adding a daemon because reclaiming once is not enough — escape: the charter's no-go on background arbitration is the reason this is a verb, and a guest already reclaims its own free memory continuously without one.
- Trimming to a target so low the guest thrashes — escape: the default target is the working set plus headroom, and the restore of headroom is part of the same operation rather than a later step.
- Reporting a drop in resident-set size as a reclaim — escape: `ADR-0082` names precisely that metric as one that reports success that did not happen, which is why item 3 comes before anything prints.
- Measuring the trim by dirtying a file on a volume — escape: that fills host page cache and measures the host, which the same record warns about; the workload has to dirty guest memory.
- Making `viv volume trim` a correctness requirement — escape: the guest already trims volumes periodically; this command shortens the wait and the documentation says so, so nobody builds a schedule around it.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, item 3's answer is recorded where the record's shape is defined rather than only here, `ADR-0035`, `ADR-0082`, `ADR-0094`, and `ADR-0113` each carry this slice and reach the status that enactment earns, the rows this slice changes are moved in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-20, before any work started, from the gap paragraph in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), which lists both reclaim verbs as designed. Unlike the other three gaps in that paragraph, no flag refuses here: the verbs do not exist in the command surface at all, so a user meets an unknown command rather than a diagnostic.

The appetite is four sessions rather than three because this is the only one of the four that needs a new path into a running guest. The other three read, refuse, or stop; this one asks the guest to give something up and then proves that the host received it, and item 3 is the reason the proof is not free.

Sequenced last of the four. It depends on [slice 025](../025-the-fleet-is-visible/README.md) for the readings its record joins, it completes the warning [slice 026](../026-a-start-checks-the-room/README.md) had to leave incomplete, and until it lands the reclaim of last resort is the stop that [slice 027](../027-the-stop-ladder-is-whole/README.md) makes whole. That ordering is also the honest one for a user: seeing the fleet, being warned about it, and being able to stop it all are each useful without this slice, while this slice is hard to price without the reporting the first one delivers.

Reshaped 2026-08-27, before any work started, by [`ADR-0113`](../../../decisions/ADR-0113-a-reclaim-verb-sits-under-its-resource.md): the memory verb is spelled `viv memory trim`, and a `viv trim` fan-out over both rungs enters as item 6 and heads the ordered remainder. The appetite does not move — the fan-out calls two rungs the slice already builds, and it is cut before anything else if four sessions bind. Goal, Core, and Acceptance changed with it, which is what this note records.

Item 3 resolved in the pair's favor, as scoping rather than amendment: `ADR-0082` rejects an unattended pair because reclamation is asynchronous, and this command is the bracket that objection says a pair lacks — it drives the balloon, waits bounded for the scope charge to settle, restores the headroom, and only then reads. The answer is recorded in [`spec/01`](../../../reference/spec/01-command-surface.md)'s reclamation-output section, the series requirement stays with the elasticity verification, and the metric half was already settled by slice 025. Two consequences of building item 4 are worth naming: the periodic in-guest trim the specification asserted did not exist until this slice landed `services.fstrim` in the guest, and the trim acknowledgement had to promise completion where the shutdown one promises motion, because the host stats the image immediately after — a second root-owned path-unit pair with a done marker carries that difference.

Host evidence, 2026-08-27, one pass each then repeated, then the full `pre-push` profile (72 of 72): `workflow_28_memory_trim_reclaims` 48.9 s then 35.3 s (dirtied guest page cache came back with `reclaimed_bytes` above zero, `viv status` showed the fall through the same reading, the detached workload survived, the immediate second trim was a `0` fact, and the fan-out's record nested both subtrees with no grand total), `workflow_28_volume_trim_returns_blocks` 23.3 s then 23.2 s (most of a written-and-deleted 512 MiB returned to the sparse image, measured on its allocated blocks and joined to `viv volume list` on both sides), `workflow_28_trim_usage_surface` at the CLI gate (the grammar, the `64`/`75`/`78` answers, the never-materialized empty record, and the fan-out's partial-failure face). The remainder was not cut: the fan-out, the `--to` floor clamp, and item 5's suggestion all landed.
