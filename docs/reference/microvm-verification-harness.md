# microVM verification harness

`scripts/first-microvm-check` builds the first microVM, boots it on a real host, and checks the things only a real host can answer. It is the base the host lane of [`testing-lanes.md`](./testing-lanes.md) grows from, and it is run by hand today.

This page is the lookup material for it: what each check proves, and the register of what has actually been verified. It holds no design rationale — that lives in the ADRs each section names.

## Running it

```console
$ scripts/first-microvm-check
```

No arguments. It resolves the flake from its own location, so it works from any working directory. Output is one `[PASS]` / `[FAIL]` / `[SKIP]` / `[RECORD]` line per check plus a verdict, and the whole run is meant to be pasted into a review.

Artefacts are retained beside the diagnostic volume under the state root: `console.log` (the raw guest console) and `memfd-series.txt` (the memory series described below). They are the reason to run it, so they outlive the run.

## The two tiers

The split is the **gate**, not the topic.

| Tier           | Gate                                                                                | What it can claim                           |
| -------------- | ----------------------------------------------------------------------------------- | ------------------------------------------- |
| **Evaluation** | Nix present                                                                         | Facts about Nix artifacts, and nothing else |
| **Host**       | `/dev/kvm`, systemd as PID 1, a systemd user manager, `$XDG_RUNTIME_DIR`, `python3` | Facts about a booted guest on this host     |

An evaluation-tier result says **nothing** about target-host behaviour. A skipped host-tier check is **unproven**, never absent. This distinction is the same one `AGENTS.md` draws when it forbids promoting an observation of an execution environment into a fact about a host.

## What the checks prove

### Purity and the build/launch boundary

- **Metamorphism** — the derivation path is unchanged by the invoking environment, the working directory, and the worktree's location on disk. Three separate checks, because each closes a different leak.
- **Negative reference** (ADR-0048) — no upstream runner or supervisor derivation is forced. vivarium generates the launch itself; forcing one of those would mean an upstream runner had written a host path into the build output, violating N5/N19.
- **Sentinel provenance** — neither launch-channel sentinel occurs anywhere in the derivation graph. Any occurrence is a defect.
- **No concrete host paths** in the graph.
- **Token flow, positively** — every scan above is negative, and a launcher that hardcoded a launch-channel value would pass all of them. The positive counterpart renders the launcher's arguments twice with different inputs and asserts each value tracked its argument.

### Backend and closure

- The backend, its control client, and the filesystem daemon are **closure members**, not `$PATH` lookups (ADR-0049).
- **Binary-cache reachability** for the hypervisor and the filesystem daemon on both supported architectures, so no user compiles a virtual machine monitor from source.

### Host tier

- **Landlock is accepted** by the pinned hypervisor.
- **Console fidelity** — see below.
- **Workspace mount contract** — the workspace is present, is the expected filesystem type, and is writable, established by reading the guest's own mount table rather than by writing a probe file into it (N9).
- **Confinement** — one hypervisor plus one filesystem daemon per share, every one owned by the invoking user with no-new-privileges set and a seccomp filter somewhere in its thread group. This is **not** the N20 allowlist test; it checks that confinement is on, not that it is correct.
- **Clean shutdown** and an empty runtime directory afterward, with the volume retained.

## Findings register

Verified on a real host. Each entry names the version it applies to; nothing here is inferred from an agent's execution environment.

### Console transport is lossless once the guest log daemon is out of the path

Routing guest output through the guest's log daemon and letting it forward to the console loses bytes: it writes each line with a single best-effort write and never retries a short one, so a congested console truncates mid-byte and reports nothing. Measured at ~0.1% of a 1 MiB burst, with a host reader attached before the guest's first write and for the whole VM life.

Writing to the console device directly makes the burst **lossless across repeated runs**, and removes the daemon's per-line prefix — cutting console traffic about 3.7x for the same payload. Decided in [`../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md`](../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md).

Two further facts, both of which cost a run to learn:

- **The console does not replay.** Bytes written while no client is attached are discarded, so a late-connecting reader gets nothing that came before it. The reader must exist not merely for the VM's whole life but _from before the guest's first write_.
- **Interleaving is not loss.** The guest init writes status lines to the same console device; one landing mid-write splits a token across two lines with every byte still present. The harness distinguishes the two and reports contention separately, because reading one as the other sends the fix to the wrong layer.

### Guest memory does return to the host

Confirmed with the metric [`../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md`](../decisions/ADR-0082-guest-memory-return-is-measured-on-the-backing-object.md) requires. A guest allocation of 512 MiB of guest RAM raised the backing object's allocated blocks by ~504 MiB; releasing it returned ~502 MiB, with proportional set size tracking the same curve. N22 and ADR-0035's headline hold on this host.

No adverse interaction was observed between reclaiming guest memory and the filesystem daemons' shared mappings of that same memory — both shares stayed functional across the reclamation and the guest completed and shut down cleanly. This was an explicitly flagged risk; it is now observed-not-reproduced rather than merely assumed.

### The shared store's validity stops at the boot closure

This is the important negative result, and it is measured as a **pair** — a single question could not have produced it:

| Question                                                                 | Answer |
| ------------------------------------------------------------------------ | ------ |
| Is a path inside the guest system's own closure valid to the guest?      | yes    |
| Is an arbitrary host store path, physically present in the share, valid? | no     |

Only the guest system's closure is registered in the guest's Nix database at boot. Every other host store path is **physically present and formally unknown**. The observed consequence: the project's own inner `nix develop` declines a path it can see (`is not valid`) and falls through to fetching it — which the boot used for this run, having no egress, could not do.

So [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md)'s benefit is proven for the boot closure and **disproven for the inner layer**.

Two later corrections to what this result was taken to mean, neither of which touches the measurement:

- **The failure is bounded by egress mode, not universal.** This boot had none. Egress is open by default ([`../decisions/ADR-0007-default-open-egress.md`](../decisions/ADR-0007-default-open-egress.md)), and an inner environment on a default sandbox substitutes normally into the writable overlay. The severity recorded here is the `allowlist`-mode severity, which the original wording did not distinguish. **Re-running this check must fix the egress mode explicitly and say which one it fixed** — a boot that silently has egress turns this finding into its opposite.
- **It is no longer a defect to close.** [`../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md`](../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md) rules that the inner layer provisions its own store and vivarium never bridges host bytes into it, so this pair now measures a **stated property** rather than a gap. It stays in the register because it is the evidence that property rests on, and because a future change that made an arbitrary host path valid to the guest would be a regression this pair detects.

The contract is in [`spec/06-workspace-and-project-environment.md`](./spec/06-workspace-and-project-environment.md).

### A guest process outside the workspace's identity map cannot use the workspace

The workspace share maps exactly one guest identity to the invoking user's, and forbids the rest — so to a guest process running as any other identity, every file in the workspace belongs to someone else. Nix's own git handling refuses to open a repository under that condition, and the refusal is what a naive probe measures instead of whatever it meant to test.

This is the identity contract in [`spec/06-workspace-and-project-environment.md`](./spec/06-workspace-and-project-environment.md) working as specified, not a defect. Anything touching the workspace must run as the mapped project identity. Relaxing the map, or suppressing the ownership check, would trade the contract for a diagnostic's convenience.

## The method note

Three of this harness's own checks have been defects of the same shape — a check whose _form_ encoded a wrong assumption, so it passed or failed for the wrong reason:

- demanding a seccomp filter of a thread-group leader that never carries one;
- a `[PASS]` that tested process exit status while claiming the guest diagnostic had completed;
- a burst-fidelity check comparing **bytes** against a threshold that the guest log daemon's own line prefixes cleared unaided, while lines were genuinely missing.

A check that fails is cheap. A check that passes vacuously, or fails for the wrong reason, sends the fix to the wrong layer — and the third one above hid a real transport defect behind a green result for two rounds. When a check's subject and its assertion can drift apart, assert on the thing the check is named after.
