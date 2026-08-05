# Resources and capacity

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

Memory and disk declarations are ceilings rather than up-front reservations. Guests report free pages so unused memory can return to the host, while sparse volume files and discard allow unused disk blocks to be reclaimed. This makes concurrent sandboxes compete for actual use rather than declared maxima.

Launch admission protects the host without creating a background arbitrator. The check considers live resource use, declared ceilings, host headroom, and backend overhead, then either admits the VM or fails before launch. Once admitted, a per-VM scope provides accounting and lifetime without hidden per-VM throttles. Reporting distinguishes configured ceilings, current use, and values that a platform does not define.

Guest memory policy takes the distribution defaults rather than adding vivarium-specific swap or virtual-memory tuning. Reclaim commands are explicit and observable: memory trim asks the guest to return unused pages, while store collection and filesystem discard have separate evidence surfaces.

Exact calculations, output fields, thresholds, and invariants N22/N23 live in [resources and capacity](../reference/spec/17-resources-and-capacity.md) and the [invariants](../reference/spec/08-invariants-and-guarantees.md).

## Governing decisions

- [ADR-0035](../decisions/ADR-0035-elastic-guest-memory-model.md) — makes a memory declaration a ceiling rather than a reservation.
- [ADR-0036](../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) — fixes launch admission and the per-VM scope, with no background arbitrator.
- [ADR-0037](../decisions/ADR-0037-volume-disk-format-and-reclamation.md) — fixes the sparse volume format that lets disk blocks return.
- [ADR-0082](../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) — fixes where a memory return is observed, and therefore what reporting can claim.
- [ADR-0091](../decisions/ADR-0091-the-store-volume-is-provisioned-for-inodes.md) — adds inode demand to the capacity a volume is provisioned for.
- [ADR-0094](../decisions/ADR-0094-guest-memory-posture-takes-the-distribution-defaults.md) — takes the distribution's memory defaults instead of vivarium-specific tuning.

## Unresolved

- [Q-003](../plan/open-questions.md#q-003--why-is-guest-userspace-boot-time-empty-and-what-metric-replaces-it-if-unavailable) and [slice 001](../plan/slices/001-close-host-measurement-gaps/README.md) own the remaining timing evidence.
