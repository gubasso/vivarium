# 026 — A start checks the room

## Goal

`viv start` spends gigabytes and minutes without ever asking whether the host has room for what it is about to boot, so the launch that pushes a machine into swap is indistinguishable from any other until the swapping starts. The same verb cannot show the boot it is performing, so a first build with nothing on the terminal reads as a hang. After this slice a launch below the reserve refuses before it builds, a launch that would overcommit the fleet says so and proceeds anyway, and `--attach` streams the console.

## Appetite

3 implementation sessions.

## Core

A launch that would leave the host below its minimum free-memory reserve refuses at `69` with nothing built and nothing booted; a launch whose fleet cost exceeds what the host has warns on stderr and proceeds, because the user decides which sandbox matters; and `viv start --attach` streams the console instead of refusing by name. The one outcome the core forbids is a refusal that arrives after the build.

## In scope

Ordered, because the seam decides where the check lives and the check decides what it reads.

1. Settle the seam between admission control and the doctor's own capacity probe. [`../../../../src/doctor/host.rs`](../../../../src/doctor/host.rs) already reads `MemAvailable` and the one-gigabyte reserve for `host-memory-headroom`, and [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) already calls that the same reading a start's admission check acts on — but the probe is soft and carries no code, so a start consumes nothing from it. Either it is promoted into the hard preflight subset with `69`, which the subset's own code set already admits, or it stays soft and admission gates separately with the catalog page amended to say which. This is a fork rather than a detail, and item 2 depends on the answer.
2. Add the fleet term. The probe knows one manifest's declared ceiling; [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) makes admission act on the measured use of the running fleet and never on the sum of declared ceilings. That measurement is [slice 025](../025-the-fleet-is-visible/README.md)'s, and admission consumes it rather than growing a second reader — which is also what keeps a single answer to what the figure measures.
3. Enact the three outcomes. Below the reserve, refuse at `69` before any build or boot. Fleet use plus the reserve over what is available, warn on stderr and proceed. Otherwise say nothing. The warning names the ways out in cost order as that page writes them, minus the reclaim verb, which does not exist until [slice 028](../028-memory-comes-back-without-a-stop/README.md) and is withheld rather than printed dead.
4. Degrade rather than skip. Where the memory controller is not delegated to the user's own manager, admission falls back to host-level readings — a weaker answer to whether there is room, and an honest one — instead of passing silently. `viv doctor` already reports the delegation shortfall, so the weaker mode is visible rather than inferred.
5. Land `--attach`. Remove the refusal in [`../../../../src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs), stream the capture that already feeds the console log ([`../../../reference/spec/16-logging-and-diagnostics.md`](../../../reference/spec/16-logging-and-diagnostics.md)), and keep the post-condition split [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) draws: the detached form returns when the guest answers and guarantees a running VM, the attached form returns when the stream ends and guarantees nothing, and interrupting the stream detaches the console without stopping the VM.

## Out of scope

- Admission at a session's cold start beyond the preflight `exec` and `shell` already share with `start`. They run the same subset today and keep running it; nothing grows a second gate.
- Disk admission. The state filesystem has its own soft probe and its own failure mode, and mixing it into a memory decision would make one refusal answer two questions.
- Reserving rather than checking. Nothing is set aside for a sandbox that has not booted; admission reads a moment and acts on it, which is what keeps the model overcommit rather than allocation.
- Attaching to a sandbox that is already running. The launch-time stream is the core; reaching an existing console is the same capture from a different starting point and is not what a first `--attach` is for.
- Ordered remainder, cut first when the appetite binds: the near-ceiling suggestion text in the warning, and the attach path's own trial beyond a demonstrated stream and detach.

## Governed by

- [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) — fixes the admission table, the reserve, and the warning's shape.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N23, which makes the pre-launch check the only moment vivarium may refuse for capacity.
- [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) — owns the probe catalog and the severity item 1 may change.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — fixes the detached post-condition and the attached form's exclusion from it.
- [`../../../reference/spec/16-logging-and-diagnostics.md`](../../../reference/spec/16-logging-and-diagnostics.md) — owns the console capture item 5 streams.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — assigns `69` and bounds the preflight's code set.
- [`../../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md`](../../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) — fixes admission control, whose scope half slice 002 realized and whose checking half this slice is.
- [`../../../decisions/ADR-0023-doctor-check-catalog-and-contract.md`](../../../decisions/ADR-0023-doctor-check-catalog-and-contract.md) — fixes the catalog contract item 1's promotion or separation sits on.
- [`../../../decisions/ADR-0035-elastic-guest-memory-model.md`](../../../decisions/ADR-0035-elastic-guest-memory-model.md) — fixes why measured use rather than declared ceilings is the quantity admission reads.

## Acceptance

If available host memory is below the reserve, then `viv start` SHALL exit `69`, and a trial SHALL assert that no build output and no unit exist afterwards — the absence demonstrated rather than assumed.

When the running fleet's measured use plus the reserve exceeds available host memory, `viv start` SHALL warn on stderr, SHALL name the sandbox it is about to start and the ways out, and SHALL boot the sandbox anyway.

When neither condition holds, `viv start` SHALL emit nothing beyond what it emits today.

When the memory controller is not delegated to the user's manager, admission SHALL still reach a decision from host-level readings, and SHALL NOT be skipped.

When `viv start --attach` is run, console output SHALL reach the terminal, and interrupting the stream SHALL leave the sandbox running, demonstrated by a `viv status` taken after the detach.

## Rabbit holes

- Making admission a scheduler because it already reads the fleet — escape: N23 forbids squeezing a running guest to make room for a new one; admission refuses, warns, or is silent, and never rearranges.
- Widening the reserve until nothing ever refuses — escape: the reserve is a specified figure, and a start that cannot be served is a refusal worth having; tuning it is a specification change with a reason, not a fix for a red trial.
- Turning `--attach` into a terminal multiplexer — escape: it streams one capture and detaches; sessions are `exec` and `shell`, which already exist and already multiplex over the control socket.
- Letting the attached form inherit the detached post-condition because both are `viv start` — escape: the lifecycle page separates them for a stated reason, and a stream that ends because the guest powered itself off must not report a running VM.
- Building a second memory reader because admission runs earlier than a status — escape: item 2 consumes slice 025's reader; two readers is how the reported figure and the acted-on figure drift apart.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, item 1's seam is recorded where the catalog defines severity rather than only here, `ADR-0036` carries this slice as the enactment of its admission half and reaches the status that enactment earns, the rows this slice changes are moved in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-20, before any work started, from the gap paragraph in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), which records that `viv start` runs no admission check and refuses `--attach`. Both are the same verb's own contract, which is why they are one slice rather than two.

Sequenced behind [slice 025](../025-the-fleet-is-visible/README.md) because the fleet term in item 2 is that slice's measurement, and behind [slice 021](../021-the-manifest-is-the-sandbox/README.md) for the reason [`../../sequencing.md`](../../sequencing.md) records for every slice that reads per-sandbox state: written before the rekey, it is written against a key that slice deletes.

Sequenced ahead of the stop and reclaim slices because N23 makes the pre-launch moment the only one at which vivarium may say no. After the boot there is no refusal left to give — only reporting, which slice 025 delivers, and reclaiming, which the user must ask for.

Item 1 exists because the probe is further along than the gap paragraph suggests. The reading, the reserve constant, and the bound manifest's ceiling are already implemented for `host-memory-headroom`; what is missing is that the probe is soft, so a start consumes nothing from it, and that it knows one manifest rather than the fleet. Naming that as a seam rather than as new work is what keeps this slice from writing a third reader of the same file.

Implemented 2026-08-26 in one pass, remainder included. The deviations and findings, each with its reason:

- Item 1's fork closed on the side the specification had already taken: [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) states that admission is a separate gate rather than a catalog probe, so `host-memory-headroom` stays soft with no code and the gate lives in `lifecycle::start`, sharing the probe's reserve constant and `/proc/meminfo` parse. The seam is recorded on the probe's own row in [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md), and the refusal's own id, `host.memory-reserve`, is named in [`../../../reference/spec/17-resources-and-capacity.md`](../../../reference/spec/17-resources-and-capacity.md) — reusing the probe id would have made a soft catalog row exit `69`.
- Item 2's fleet term reads a wider domain than `viv status -g`: the union of the workspace index and the live runtime target directories, because a running VM whose manifest was removed still consumes memory, while the report's enumeration domain deliberately keys on the library (spec/01). The readers themselves are slice 025's, unchanged; a partial fleet sum is carried as a lower bound and the warning says `at least`.
- The warn tier's evidence is a unit-test matrix rather than a host trial: this host does not delegate the memory controller (`host-cgroup2-delegation` trips), so the fleet term is unmeasurable here and an end-to-end warn trial would skip vacuously — the harness method note's lesson. The refusal trial doubles as the degraded-tier demonstration for the same reason: its decision was reached from host-level readings on exactly such a host.
- The refusal trial arranges the low reading by bind-mounting a crafted `meminfo` over `/proc/meminfo` inside a private user+mount namespace, so the product path keeps its one reader and no test seam entered the tree; the trial prechecks the kernel's willingness to bind and fails by name rather than skipping.
- `--attach` streams by following `console.log` — the capture the supervisor tees — with a rotation-aware follower; the serial socket stays the supervisor's alone. The terminating-signal watchers are installed before the blocking boot: the first trial run showed a `SIGINT` sent mid-boot killing the process under the default disposition, so the attached form now consumes it and answers with a detach the moment the stream opens, at the stated cost that aborting an attached build takes more than `Ctrl-C`. Detach exits `0` and the other terminating signals report `128+S` (spec/10 amended); `--attach --json` is refused as usage because a byte stream has no record to promise (spec/01 amended).
- Attaching to an already-running sandbox stayed out of scope as shaped: the attached form delivers the already-running note, says the launch-time stream attaches only to a boot it performed, and streams nothing; [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) records the residual against spec/10's design sentence.
