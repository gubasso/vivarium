# ADR-0082: Guest memory return is measured on the backing object

## Context and Problem Statement

N22 and [`ADR-0035-elastic-guest-memory-model.md`](./ADR-0035-elastic-guest-memory-model.md) promise that guest memory the workload stops using comes back to the host. Nothing said how that is observed, and the obvious metric is wrong. Guest RAM is shared with the filesystem daemons, so it is backed by a file object mapped shared; releasing a range unmaps it from the hypervisor's page tables before the pages are freed. Resident-set size therefore falls the instant the hypervisor advises the range away, whether or not a single page was returned — a metric that reports success that did not happen.

## Considered Options

- Hypervisor resident set size — the reachable, obvious number.
- Guest-side available memory — cheap, but says nothing about the host.
- Allocated blocks of the backing memory object, with proportional set size as a cross-check.

## Decision Outcome

Chosen option: allocated blocks of the backing memory object.

- It moves only when the hole-punch that actually frees pages succeeds, which is the property N22 asserts. Resident-set size and guest-side counters both move for reasons unrelated to it.
- Proportional set size is the cross-check, not the result. Agreement between the two raises confidence; disagreement means the mapping changed without the allocation changing, and the allocation wins.
- Sampled as a series across the VM's life, not as a pair of readings. A before/after pair cannot see a transition it did not bracket, and reclamation is asynchronous — the guest hints free pages, the hypervisor acts later.
- The guest announces phase transitions so the host series can be aligned to them. A host sampler that cannot tell allocation from release is measuring weather.
- The workload must dirty guest RAM, not a file on a volume, or it measures host page cache instead.

## Consequences

- Good: N22 becomes falsifiable rather than merely plausible.
- Good: the same series shows when memory returns, not only whether.
- Bad: needs a host sampler whose lifetime is independent of the guest workload.
- Bad: the metric is less familiar than resident-set size, so it must be labelled wherever it is reported.

## Status

Implemented

Gives [`ADR-0035-elastic-guest-memory-model.md`](./ADR-0035-elastic-guest-memory-model.md) the method that verifies its headline; the reading itself is recorded in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md). Implemented by the sampler in [`../../tests/host/base-image-check`](../../tests/host/base-image-check) and the guest phase markers in [`../../tests/nix/measurement/diagnostic.sh`](../../tests/nix/measurement/diagnostic.sh), where the harness moves this record's original enactment paths. The reported `mem_used_bytes` figure consumes this record through the scope's memory charge, which moves only on the actual free of the backing object's pages; the reconciliation and the labeling this record demands are written in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md)'s reporting section.

Consumed also by slice [`028-memory-comes-back-without-a-stop`](../plan/slices/028-memory-comes-back-without-a-stop/README.md) — 2026-08-27, whose `viv memory trim` reads a before/after pair of that same charge. The pair is admissible against this record's series requirement because the command supplies the bracket an unattended pair lacks; the scoping is recorded in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md)'s reclamation-output section, and the series requirement stays with the elasticity verification this record was written for.
