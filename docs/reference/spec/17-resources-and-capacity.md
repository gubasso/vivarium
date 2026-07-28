# 17 — Resources and capacity

Why you never size a VM, what a running VM actually costs the host, and what happens when several run at once. The governing decisions are [`../../decisions/ADR-0035-elastic-guest-memory-model.md`](../../decisions/ADR-0035-elastic-guest-memory-model.md) (guest memory), [`../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md`](../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) (host scoping and admission), and [`../../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../../decisions/ADR-0037-volume-disk-format-and-reclamation.md) (disk). The volume _model_ those sizes apply to is owned by [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md).

## Ceilings, not reservations

Every resource figure vivarium accepts or computes is a **ceiling**: the most a VM may use, not an amount taken from the host on its behalf (N22, [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)). This holds whether the value was declared in `[resources]` ([`03-artifact-model.md`](./03-artifact-model.md)) or filled in by the policy below.

- **Memory** is demand-faulted. A VM declared 8 GiB starts at a fraction of that and grows toward it only as the guest actually touches pages.
- **Memory is returned.** The guest reports pages it has finished with, and the host reclaims them, so resident size tracks the working set rather than ratcheting to the ceiling.
- **vCPUs are schedulable threads**, not reserved cores. An idle VM's vCPUs cost approximately nothing; the host scheduler overcommits them exactly as it overcommits any other threads.
- **Volume size is virtual.** The image occupies what its contents occupy.

The consequence users feel: the sum of ceilings across running projects may exceed the host's memory and disk. The sum of _measured_ use may not — and that is what admission control and `viv status` report on.

The limits of the model are stated plainly, because they are the two ways a user can still be surprised:

- Free-page reporting returns large blocks of genuinely free guest memory. It does **not** return the guest's page cache, which a build or a repository-wide search fills. Resident size therefore drifts upward over a long session; `viv trim` is the answer, and it is never automatic.
- A ceiling is real. A guest can exhaust its own ceiling while the host has memory free. That is the price of a bounded VM, and it is why the default ceiling is generous.

## Auto-sizing

`resources.mem_mib` and `resources.vcpu` are optional and render as `null` when undeclared ([`01-command-surface.md`](./01-command-surface.md)). Undeclared does not mean unset — it means resolved at launch from the host:

| Knob                | When undeclared                                                                    |
| ------------------- | ---------------------------------------------------------------------------------- |
| `resources.mem_mib` | half of host physical memory, rounded down to a whole GiB, clamped to **4–16 GiB** |
| `resources.vcpu`    | the host's CPU count, capped at **8**                                              |
| volume size         | **32 GiB** virtual per volume                                                      |

Half of host memory is safe _because_ it is a ceiling. The lower clamp keeps a real toolchain from thrashing on a small host; the upper clamp stops one runaway project from being able to consume a large host by itself. A declared value always wins and is still a ceiling.

These are **launch-time** values derived from the host, so they are never build inputs (N3, N19, [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). Two hosts running the same manifest build the same VM and boot it with different ceilings.

## What a VM costs the host

A VM is not one process. Every process vivarium starts on its behalf — the VMM, one filesystem daemon per share, launch helpers — is placed in **one host resource scope per (project, target)**, nested under a single vivarium slice.

The scope carries **accounting, a relative CPU weight, and no memory limit**. A per-VM memory limit would make the host reclaim pages the guest believes are resident, working against the guest's own cooperative return of memory, and a hard limit can only kill the monitor process — taking every session in that project with it. The vivarium slice carries no limit either: it groups the fleet, and is where any future whole-fleet backstop would go.

What the scope buys, in exchange for nothing:

- **A real cost figure per VM**, covering the monitor and every filesystem daemon together — the `mem_used` column below.
- **A pressure reading per VM**, reported and never acted upon.
- **Whole-VM teardown**: `viv stop` releases the scope, so no daemon can outlive the VM it served.
- **Fair CPU between projects** without pinning anything.

**I/O weighting is deliberately absent.** A per-user scope can carry memory accounting and a CPU weight, but the I/O controller is not delegated to a user's own resource manager — so an I/O weight set there would be silently inert. Claiming it would be worse than not having it. Projects therefore compete for disk on the host's ordinary terms.

**Where the host delegates nothing, the model degrades rather than fails.** Per-VM accounting needs the memory controller delegated to the user's manager. Where it is not, vivarium still launches: teardown works regardless, `mem_used` reports as unavailable rather than zero, and admission control falls back to **host-level** memory readings — a weaker but still honest answer to "is there room?". `viv doctor` reports the shortfall (`host-cgroup2-delegation`) so the degradation is visible rather than silent.

## Admission control

`viv start` checks host capacity before it builds or boots (N23). It uses **measured** use of the running fleet, never the sum of declared ceilings:

| Condition                                                      | Outcome                                                                 |
| -------------------------------------------------------------- | ----------------------------------------------------------------------- |
| Available host memory below the minimum reserve (**1 GiB**)    | **Refuse**, exit `69` — nothing is built or booted                      |
| Fleet's measured use plus the reserve exceeds available memory | **Warn on stderr and proceed** — the user decides which project matters |
| Otherwise                                                      | Proceed silently                                                        |

The warning names the situation and the three ways out, in cost order:

```text
warning: 4 project VMs are running and using 21.3 GiB of 31.2 GiB host memory.
         Starting `api-gateway` may push the host into swap.
         viv trim          reclaim cached guest memory in this project
         viv status -g     see what is running
         viv stop          stop a project you are done with
```

This is the whole arbitration story. vivarium reports pressure and lets the user act; it never squeezes a running guest to make room for a new one, and it runs no background process to decide. `viv doctor` surfaces the same host readings as ordinary checks ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)).

## Reclaiming memory: `viv trim`

`viv trim [--to <MiB>]` is the one command that reclaims memory on demand, and it exists because free-page reporting cannot return page cache. It briefly asks the guest to give back memory down to the target, then immediately restores the guest's headroom, and reports what the host got back. It is bounded, synchronous, and always user-invoked.

- With no `--to`, the target is the VM's measured working set with headroom — enough to drop cache, not enough to disturb running work.
- It requires a running VM; on a stopped one it exits `75`, the same "stop first / start first" category `volume rm` uses ([`14-exit-codes.md`](./14-exit-codes.md)).
- It is safe to run against a busy VM: the guest reclaims its own caches first and can take memory back immediately if it needs it.

`viv volume trim [<name>]` is the disk counterpart: it returns space freed inside a volume to the host image. Volumes are also trimmed periodically inside the guest, so this is for impatience, not correctness.

`viv status` suggests `trim` when a VM's measured use approaches its ceiling while host memory is low. It is a suggestion on stderr, never an action.

## Reporting

`viv status` and `viv status -g` show the ceiling and the reality side by side, for both memory and disk. That contrast is the whole model in one table:

```text
PROJECT          STATE     MEM (used/ceiling)   VCPU  DISK (alloc/virtual)  SESS  UP
api-gateway      running    2.1 GiB / 8 GiB      8     4.2 GiB / 32 GiB      3    3h12m
web-frontend     running    5.8 GiB / 8 GiB      8    11.7 GiB / 32 GiB      1    1h04m  (near ceiling)
data-pipeline    running    1.4 GiB / 8 GiB      8     2.9 GiB / 32 GiB      2      22m
infra-tf         stopped         -  / 8 GiB      -     0.8 GiB / 32 GiB      -       -

host: 9.6 GiB available of 31.2 GiB - memory pressure (60s): 0.4%
```

- **used** is the scope's current memory: the monitor plus every filesystem daemon for that VM.
- **alloc / virtual** is the volume image's allocated size against its declared virtual size.
- **sessions** is the count of attached `exec`/`shell` sessions ([`12-exec-and-shell.md`](./12-exec-and-shell.md)).
- Stopped projects report no live figures but still report allocated disk, because volumes persist across `stop` (N18).

The `--json` shape adds a `runtime` object beside the existing declared `resources`, so a consumer can tell a declaration from a measurement; the record is fixed in [`01-command-surface.md`](./01-command-surface.md).

## Guest-side posture

Two guest defaults follow from the model and are part of the base image, not user knobs:

- **A small compressed in-memory swap device.** A session that briefly overshoots compresses cold pages instead of losing a process. Compressed pages remain guest memory, so nothing reaches host storage.
- **No disk-backed swap in the guest.** It would convert guest memory pressure into block I/O into host page cache — the worst of both, multiplied by the number of running VMs.

Guest memory overcommit stays at the kernel default. Strict accounting with no swap would spuriously fail the large, short-lived allocations that toolchains and language servers make constantly.

## Capacity in practice

The design target is a single developer workstation running **four or five project VMs at once**, each with several attached sessions. At that scale the fixed per-VM overhead — a guest kernel, the monitor process, one filesystem daemon per share — is real but small against the working sets, and elasticity does the rest.

This is deliberately not a fleet scheduler. vivarium is not an orchestrator ([`00-goals-and-non-goals.md`](./00-goals-and-non-goals.md)); at a scale where automatic arbitration between dozens of guests would be required, the honest answer is to stop a project rather than to squeeze one.
