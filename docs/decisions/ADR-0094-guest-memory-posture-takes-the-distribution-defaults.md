# ADR-0094: The guest's memory posture takes the distribution defaults

## Context and Problem Statement

[`spec/17`](../reference/spec/17-resources-and-capacity.md) specifies a guest-side memory posture — a compressed in-memory swap device, no disk-backed swap, kernel-default overcommit — and the base image implements none of it. Four settings are in question: the swap device and its size, the two swap-tuning sysctls, proactive compaction, and the page-reporting order.

## Considered Options

- Enable the distribution's own zram module at its shipped defaults and tune nothing else.
- Enable zram at a smaller, vivarium-chosen size, because swapped pages are guest memory the host cannot reclaim.
- Set size, `vm.swappiness`, `vm.page-cluster` and `page_reporting_order` from first principles.

## Decision Outcome

Chosen option: **the distribution defaults** — one zstd zram device sized at half of observed RAM, and no sysctl vivarium picks itself.

The second option rests on a premise that does not hold. A zram device's declared size is a **ceiling on compressed capacity, not a reservation**: unused it costs about 0.1% of that size in metadata, and it grows only under the pressure it exists to absorb. That is N22's own distinction, so taking the default agrees with the elastic model rather than straining it.

The swap-tuning pair is settled by a search that failed. No upstream source states a posture for a virtual-machine guest — not the kernel's zram documentation, not the distribution module, not the swap generator's own manual. Rather than dress a guess as a citation, vivarium sets neither and inherits what every other Linux system runs.

## Consequences

- Good: every value is upstream's, so each is defensible by citation and moves when upstream moves.
- Good: `page_reporting_order` stays unset, which is correct rather than lazy — on the primary target it resolves to `pageblock_order`, already 2 MiB, so writing it changes nothing. Load-bearing only on 64K-page arm64.
- Good: proactive compaction is on by default, so `spec/17`'s compaction claim becomes true with no setting — deliberate rather than accidental.
- Bad: half of RAM is not "small", so `spec/17`'s wording changes with this record.
- Bad: the swap-tuning pair stays **policy by omission, unverified** — a real gap, recorded as one.

## Status

Accepted
