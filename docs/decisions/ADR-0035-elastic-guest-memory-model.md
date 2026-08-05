# ADR-0035: Elastic guest memory — ceilings, not reservations

## Context and Problem Statement

`[resources] mem_mib` and `vcpu` are nullable, with no documented default and no mapping to backend arguments. A developer working across four or five projects at once cannot be asked to hand-size each VM, and guest memory that only ever grows becomes a de-facto reservation: five 8 GiB VMs would commit 40 GiB of a 32 GiB host while idle.

## Considered Options

- Fixed per-VM memory, user-declared (the status quo).
- A fixed ceiling plus a balloon device with free page reporting.
- Memory hotplug (virtio-mem) sized on demand.
- A host-side controller that inflates balloons under host memory pressure.

## Decision Outcome

Chosen option: fixed ceiling plus free page reporting — the guest hands unused pages back to the host on its own, so declared memory bounds a VM without committing host RAM.

- Declared memory is a ceiling, never a reservation (N22). Guest RAM is demand-faulted; the host pays for the working set, not the declaration.
- Undeclared `mem_mib`/`vcpu` resolve through the auto-sizing policy in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md); explicit values are ceilings too.
- The balloon is launched at zero size with free page reporting and deflate-on-OOM enabled. Reporting is independent of balloon size, so nothing is taken from the guest to obtain it.
- No autoscaler. Reclaiming a running guest's memory from the host converts memory pressure into sustained page-fault storms. The only inflation is the bounded, user-invoked `viv trim`.
- virtio-mem is rejected: it re-bases what declared memory means, needs guest-kernel support vivarium does not own, and shrinks the address space when the scarce resource is host memory.

## Consequences

- Good: concurrent VMs cost their working sets; the user never sizes a VM.
- Bad: resident size still drifts upward, for two reasons: the guest page cache is not free memory, so reporting never returns it; and reporting yields only large blocks, so fragmented free pages stay held. `viv trim` answers the first.
- Bad: the ceiling is real, so a guest can exhaust it while the host has memory free.

## Status

Accepted

Amended by ADR-0094 — `page_reporting_order` is left unset and resolves to `pageblock_order` on the primary target, and proactive compaction is the distribution default rather than a vivarium setting; the paragraph below claiming an explicit base-image setting does not survive reading the kernel.

Amended by ADR-0049 — the balloon-driver requirement is an evaluation-time assertion on the guest vivarium builds, not a `doctor` probe, and the backend version floor that free page reporting needs is settled by the lockfile rather than checked at runtime.

The zero-size balloon is settled rather than pending: the backend's own configuration parser defaults the size to zero and validates only against exceeding guest RAM, its device construction advertises free page reporting from that feature's own flag independently of size, its documentation uses a zero-size reporting balloon as its worked example, and the guest module's corresponding option is an unsigned integer defaulting to zero. The device itself must still exist, because the runtime resize `viv trim` performs is refused when there is no balloon device to address.

The guest kernel's page-reporting granularity is an explicit setting of vivarium's own base image rather than an inherited default, and the guest compacts proactively so that fragmented free memory becomes reportable without host involvement — which is guest self-reclamation under N22, not the host arbitration N23 forbids.

Note on the rejected controller — the controller designs that outperform an explicit `viv trim` all decide from a per-guest working-set signal reported by the balloon driver, which tells the host which pages are cold. vivarium's backend exposes no such signal. A controller here could act only on a pressure scalar, which is the shape that produces the fault storms this decision rejects.

Discharges the resource-knob mapping left open by [`ADR-0025-default-hypervisor-cloud-hypervisor.md`](./ADR-0025-default-hypervisor-cloud-hypervisor.md), which it amends with the required backend memory arguments. Adds N22 to [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md); the policy, verbs, and status surface live in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).
