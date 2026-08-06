# ADR-0036: Host resource scoping and admission control

## Context and Problem Statement

ADR-0027 lists "cgroup limits" in the launch profile without saying what they limit or to what value. Nothing stops a fifth `viv start` on a host that cannot hold it, and `viv status` cannot report what a VM costs, because a VM is not one process: it is the VMM plus one virtiofsd per share.

## Considered Options

- Leave cgroup use unspecified and let the implementation choose.
- A per-VM hard memory limit (`MemoryMax`) enforcing the declared ceiling on the host.
- A per-VM scope used for accounting, weighting, and teardown, with no per-VM limits, plus a launch-time admission check.
- A host daemon that watches pressure and arbitrates between running VMs.

## Decision Outcome

Chosen option: a per-VM scope for accounting, with no per-VM limits and a launch-time admission check.

- Every VM's processes — the VMM, every per-share virtiofsd, launch helpers — are placed in one transient systemd user scope per (project, target), nested under a single `vivarium` slice.
- The scope carries accounting, a CPU weight, and nothing else. There is deliberately no per-VM memory limit: it would force host reclaim of memory the guest believes resident, fighting the cooperative balloon (ADR-0035), and can only kill the VMM, taking every session with it. The `vivarium` slice groups only; a future fleet backstop would go there. No I/O weight — that controller is not delegated to a user's manager, so one set there would be inert.
- The scope is the measurement surface: current memory and pressure per VM are read from it rather than derived from process tables.
- `viv start` runs an admission check (N23): refuse below a minimum host reserve, warn and proceed when the fleet's measured use plus the reserve exceeds available memory.
- Pressure information is read and reported, never acted on. No arbitration daemon.

## Consequences

- Good: teardown becomes one operation; per-VM cost becomes observable for free.
- Good: overcommit stays the user's informed choice.
- Bad: per-VM measurement needs a delegated cgroup v2 hierarchy. Without one the model degrades — teardown still works, admission falls back to host-level readings — rather than refusing to launch.

## Status

Accepted

Amends [`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) by giving its "cgroup limits" concrete content. Adds N23 to [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md); the check, thresholds, and reported fields live in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).

Amended by [`ADR-0097`](./ADR-0097-the-transient-user-service-owns-the-vm-lifetime.md) — the per-VM resource scope is a manager-owned transient user service rather than a caller-owned `.scope` unit.
