# ADR-0085: A running guest pins the store paths it reads

## Context and Problem Statement

[`ADR-0038`](./ADR-0038-guest-store-sharing.md) shares the live host store read-only, making it an overlayfs **lower layer** inside the guest, and [`ADR-0039`](./ADR-0039-share-cache-policy.md) grants it unbounded caching because store contents are immutable. A host garbage collection under a running guest falsifies both. ADR-0038 owns no mechanism.

## Considered Options

- **Document only** — state the rule; rely on the user.
- **Detection only** — verify a running guest's paths still exist.
- **Keep the closure pinned for the VM's lifetime, plus a detection check.**
- **A per-VM store image** — ADR-0038's rejected alternative, immune by construction.

## Decision Outcome

Chosen option: **keep it pinned, plus a `doctor` check.**

- **Documentation cannot hold, because vivarium triggers the hazard itself.** Nix collects garbage mid-**build** when free space falls below `min-free` ([Nix manual](https://nix.dev/manual/nix/2.31/command-ref/conf-file)), so `viv build` can collect paths another project's running guest reads. A rule the tool breaks for the user is not a rule.
- **No new root is taken.** A running VM boots from a generation, and every generation symlink is already a garbage-collector root (N14, [`ADR-0014`](./ADR-0014-build-generations-and-gc-roots.md)), so the closure is pinned before this decision says anything.
- **The mechanism is a refusal, not a root.** `viv generations prune` must not unlink the generation a running VM booted from — read from the boot record, never from `current`, which `--generation <n>` and a stale VM both diverge from.
- **Detection is the second half, not an alternative.** A pin cannot stop an `rm -rf`, nor collection outside the closure, so `doctor` reports when a running guest's closure is not intact — ADR-0038's contract stands, and this stops vivarium breaking it.
- **There is no duplication fallback.** [`ADR-0086`](./ADR-0086-per-vm-store-duplication-is-refused.md) withdraws ADR-0038's reservation: an insufficient pin means a better interlock, never duplication.

## Consequences

- Good: no new durable state is written, and nothing is released at `viv stop`, so a crash leaves no leftover to reap.
- Bad: a running VM's generation cannot be pruned until it stops.
- Bad: two mechanisms, since prevention and diagnosis cover different faults.

## Status

Accepted

Discharges the host-GC interlock obligation stated in [`ADR-0038`](./ADR-0038-guest-store-sharing.md)'s `## Status` and in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md). The `doctor` check is `store-roots-intact`, specified in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md); the prune refusal is in [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md).

**Amended 2026-08-03, before implementation.** The decision survived; the mechanism first written for it did not. That mechanism was a **second** garbage-collection root, taken at `viv start` and released at `viv stop` — redundant with N14, which has pinned every retained build since [`ADR-0014`](./ADR-0014-build-generations-and-gc-roots.md), and the one part of the design that could leave a durable leftover, because a root released by a command that may never run outlives the crash that skipped it. The correction is strictly smaller than what it replaces: the closure is pinned already, so what was missing is a guard on **unlinking** the pin. `viv destroy` needs none — it stops the VM before tearing down ([`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md)) — but `viv generations prune` had no running-VM guard, where `viv volume rm` and `volume prune` already refuse with `75`. That asymmetry was the actual defect, and it predates this decision.

**What the guest observes when the rule is broken is unmeasured, and one measurement can still move the check's trigger.** The kernel's contract is that overlay behaviour becomes undefined, not that any particular error is returned, so the symptom has to be observed on a real host rather than predicted. It matters because making `store-roots-intact` a `doctor` check is a **bet that the symptom is loud**: the check attributes a fault the user has already noticed, so it only has to run when they ask. `ESTALE`, an `open()` failure, or a hang keep that bet — the user sees something wrong and reaches for `doctor`. A **successful read of freed blocks** loses it outright: nothing fails, a build consumes wrong bytes and reports success, and no one is ever prompted to invoke a check that runs only on demand.

**Measured 2026-08-04, and the bet holds.** `scripts/store-gc-interlock-check` ran the experiment on a real host — the guest read a store path in full, the host collected it mid-run, the guest read again — with a never-touched control path proving the deletion propagated and a before-phase read proving something was cached to invalidate. Results are in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md). The class is **mixed, and never corrupt**: every read that succeeded returned bytes identical to the host's, so the outcome this decision could not survive — a successful read of _freed_ blocks — did not occur.

The split is by access shape, and it is mechanical. virtiofsd holds a descriptor per inode the guest knows, so while the guest still holds a path open it keeps reading the real bytes; once the guest forgets the inode, a lookup by name reaches a host path that is gone and returns `ENOENT`. The realistic hazard — a build resolving a path it does not already hold open — is therefore **loud**, which is exactly what makes `store-roots-intact` viable as an invoked-only `doctor` check. **No amendment to the trigger follows, and the mechanism above is unchanged.**

Two residuals, recorded rather than acted on. A process that already holds the file open reads correct bytes indefinitely, which is not a correctness fault. And a `readdir` after the guest forgets the directory returns _no entries_ while a read by name of an omitted entry still succeeds — an incoherent view that is the kernel's documented "undefined", not any particular error. `mmap` remains unmeasured; the harness page carries that as a stated gap.

The paragraph below is kept for the reasoning it records, and is now counterfactual: if the symptom proves silent, the amendment is to the **trigger, not the mechanism**: `store-roots-intact` stops being invoked-only and becomes something that runs unprompted — periodically per running VM, or in the preflight of the commands that touch a guest. The root in the decision above is unaffected either way. The measurement is carried in the design checklist, together with the check-design trap it implies — removing a path the guest has never read proves nothing, because nothing was cached to go stale — and with the premise this decision now rests on: that the guest's kernel and system are store _references_ of the generation's output, so the generation root pins them transitively. If they are named at runtime rather than referenced, the reuse above does not hold and the mechanism returns to being an open question.
