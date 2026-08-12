# 014 — Workspace and persistence

## Goal

Give the sandbox the project tree and a home that survives a restart, so that the milestone this chain aims at is reached: a microVM a coding agent can be run inside.

## Appetite

2 implementation sessions.

## Core

The project tree is mounted read-write at the specified path and an edit made inside the guest is visible on the host, the guest home survives a `viv stop` followed by a `viv start`, and `viv destroy` removes what it owns so the next `viv start` is a cold rebuild.

## In scope

Ordered, because a workspace the guest cannot write to makes every persistence question untestable.

1. Mount the project tree read-write at the specified path and confirm an edit crosses the boundary in both directions.
2. Settle whether the share identity translation and the host link-farm masking are correctness requirements for this mount rather than robustness polish. If either is, it belongs in [slice 012](../012-first-boot/README.md)'s `Governed by` and is recorded there; the answer is owed before item 3.
3. Back the guest home with a volume that survives stop and start.
4. Implement `viv volume` list and the prune boundary the volume model fixes.
5. Implement `viv destroy` and its cold-rebuild boundary.
6. Land the three `Virtualization` trials this slice's `Acceptance` names, unskipped, and restore the pre-push gate as the last of them goes green. `profile.pre-push` selects `kind(test) - binary(user_workflows)` because the acceptance trials assert verbs slices 013 and 014 implement; this slice is where that whole-binary exclusion narrows to the one trial still waiting on a verb, `- test(=workflow_05_restrict_egress_allowlist)`, which [slice 004](../004-enforce-egress-allowlist/README.md) removes when it enforces the allowlist. Narrowing rather than deleting, because the binary carries trials this slice greens and one it cannot: dropping the clause outright would put the stage straight back to failing on a capable host. [`../../sequencing.md`](../../sequencing.md) owns the obligation and carries the measurement to replace.

## Out of scope

- Store reclamation under space pressure, and any collection behavior beyond what `viv destroy` removes.
- Generations and `viv update`.
- Secrets and config sharing beyond the credential relay already proved.
- Extra mounts past the project tree and the home. The manifest may declare them; exercising more than the core needs is remainder.
- Ordered remainder, cut first when the appetite binds: `viv gc` store sweeping, `viv trim`, and volume format migration.

## Governed by

- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — defines the workspace mount and its confinement.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — defines the stop, start, and destroy boundaries.
- [`../../../explanation/guest-store-and-volumes.md`](../../../explanation/guest-store-and-volumes.md) — owns the volume topology.
- [`../../../explanation/shared-filesystems.md`](../../../explanation/shared-filesystems.md) — owns the share behavior item 1 depends on.
- [`../../../decisions/ADR-0019-volume-model.md`](../../../decisions/ADR-0019-volume-model.md) — fixes the volume model.
- [`../../../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../../../decisions/ADR-0037-volume-disk-format-and-reclamation.md) — fixes the disk format and reclamation boundary.
- [`../../../decisions/ADR-0043-identity-marker-lifecycle.md`](../../../decisions/ADR-0043-identity-marker-lifecycle.md) — fixes what `viv destroy` does to the marker.
- [`../../../decisions/ADR-0066-share-uid-gid-translation.md`](../../../decisions/ADR-0066-share-uid-gid-translation.md) — fixes identity translation across the share; item 2 decides which slice owns it.
- [`../../../decisions/ADR-0067-volume-prune-and-first-boot-home.md`](../../../decisions/ADR-0067-volume-prune-and-first-boot-home.md) — fixes prune and the first-boot home.
- [`../../../decisions/ADR-0080-the-sandbox-is-disposable.md`](../../../decisions/ADR-0080-the-sandbox-is-disposable.md) — fixes what may and may not be treated as a system of record.
- [`../../../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md`](../../../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md) — fixes the masking; item 2 decides which slice owns it.

## Acceptance

When the guest writes to the workspace mount, the host SHALL observe the change at the project tree, and the reverse SHALL hold.

When a sandbox is stopped and started again, its home volume SHALL retain what was written before the stop and `workflow_07_stop_restart_preserving_volumes` SHALL pass unskipped.

When `viv stop` is issued, shutdown SHALL complete inside the specified budget and `workflow_07_stop_completes_within_budget` SHALL pass unskipped.

When `viv destroy` completes, the next `viv start` SHALL be a cold rebuild and `workflow_08_destroy_cold_rebuild` SHALL pass unskipped.

If item 2 finds that identity translation or link-farm masking is a correctness requirement for first boot, then that decision SHALL be recorded in slice 012's `Governed by` and in this slice's `Revisions`.

## Rabbit holes

- Treating guest durability as a system of record — escape: the sandbox is disposable, so persistence here means a home that survives a restart, not data the user may rely on.
- Chasing store reclamation because a volume raised the question — escape: reclamation under pressure is a decided but unfunded cluster; record the observation and leave it there.
- Answering item 2 by moving the ADRs without evidence — escape: the answer is whether first boot is wrong without them, and that is observable from slice 012's booted guest.
- Growing the mount surface to whatever the manifest can declare — escape: the project tree and the home are the core; anything else is remainder.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, a coding agent has been run inside the sandbox against the mounted project tree at least twice, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved to Implemented, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
