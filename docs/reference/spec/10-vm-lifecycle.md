# 10 — VM lifecycle: `viv start`, `viv stop`, and teardown

How `viv start` takes a project from nothing to a running sandbox, how `viv stop` and `viv destroy` bring it back down, and the rules that make each safe to run twice. The lifecycle decision and rationale are in [`../../decisions/ADR-0013-vm-lifecycle-and-up.md`](../../decisions/ADR-0013-vm-lifecycle-and-up.md) (which records `start` under its former name `up`) and [`../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md); the output-stream and failure conventions used below are in [`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).

## Lifecycle states

A project's VM is in one of six states, surfaced by `viv status` ([`01-command-surface.md`](./01-command-surface.md)):

- **absent** — never built: no store output and no runtime state.
- **built** — a sandbox output exists in the store but no VM is running. A clean `viv stop` lands here; this _is_ the "stopped" state.
- **starting** — the VMM has been launched but the guest is not yet live (the control-socket liveness handshake does not answer yet). Transitional. Because `start` returns only once the guest answers (see _What `viv start` does_), this state is observable to a **concurrent** `viv status` while another process holds the per-target `flock` and is still booting ([`12-exec-and-shell.md`](./12-exec-and-shell.md)) — never in the window after your own `start` returned.
- **running** — a VM is up for this project and answering. It carries a **`stale`** condition — a boolean, not a separate state — true when a layer changed so the current build's store output no longer matches what the running VM was booted from. Freshness is the store output path, never a separate digest (N4, [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)); a stale VM is still **running**, only drifted.
- **stopping** — transitional, passed through by `viv stop` and `viv destroy`; never a resting state.
- **failed** — vivarium cannot treat the VM as healthy: boot failed, the VMM or guest exited abnormally, or a runtime record is broken. The specific cause is carried as a _reason_ (`crashed`, `boot-timeout`, `socket-lost`, …), not a separate state.

The transitions:

```text
absent | built ──start──▶ (starting) ──▶ running
running ──stop──▶ (stopping) ──▶ built
running | built | failed ──destroy──▶ built (store paths linger) ──gc──▶ absent
running | starting ──abnormal exit──▶ failed
```

`stop` can only ever reach **built** — a stopped project's build output stays pinned. A clean `stop`/`destroy` **tears down the runtime markers** (stale pid, socket), so "markers present but the process is dead" is exactly what separates **failed** from **built** — the invariant `viv status` relies on to report a crash rather than a clean stop. A project returns to **absent** only when `destroy` has unlinked its generations _and_ a later collection has reclaimed the store paths ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).

## What `viv start` does

`viv start` drives the project toward **running**, in this order, failing closed at the first unmet step (see _Preflight_ below):

1. **Resolve** the bound manifest by the precedence in [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md). None resolves → fail closed (N7).
2. **Preflight** the host — refuse before any build or launch if a hard prerequisite is missing.
3. **Admit** — check host capacity against the measured cost of the VMs already running: refuse below the minimum free-memory reserve, warn and continue when the fleet makes this VM a risk (N23, [`17-resources-and-capacity.md`](./17-resources-and-capacity.md)). Like preflight, this runs **before any build**, so a host that cannot hold the VM never pays for one.
4. **Evaluate and build** the outer flake to a store output. Identical inputs reuse the cached output (N4); the result is recorded as a **generation** ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).
5. **Ensure running** — boot the VM if one is not already up for this project, injecting the workspace host path at launch (N5) and mounting it read-write at the fixed guest path ([`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md)). Host-derived resource ceilings are resolved and applied at this moment for the same reason the workspace path is (N3, N19), and every process launched for the VM is placed in the target's host resource scope. This step also **persists the project's identity marker** if the project does not have one yet ([`15-project-identity.md`](./15-project-identity.md)). It then **waits for the guest to answer** the control-socket liveness handshake specified in [`12-exec-and-shell.md`](./12-exec-and-shell.md), which is what makes a boot timeout detectable as `69` ([`14-exit-codes.md`](./14-exit-codes.md)).

**Post-condition.** `viv start` without `--attach`, exiting `0`, guarantees the VM was **running** at the moment it returned — never still `starting`. The guarantee is point-in-time, not durable: a guest that crashes immediately afterwards makes `failed` a correct next reading. `--attach` is excluded because it returns when the console stream ends rather than when the VM is ready (see _Attach vs detach_).

`start` is **idempotent**: on a fresh, already-running VM it is a no-op that exits `0` (N15). This "ensure running" step is the shared routine `viv exec` and `viv shell` reuse when they start the VM if needed.

`exec` and `shell` call the same ensure-running routine. When the control-socket ping proves the VM is already running for the same project — identified by the key in [`15-project-identity.md`](./15-project-identity.md) — they skip preflight/build/boot and attach a session. When they must cold-start, they run the same hard preflight subset as `start` before any build or launch.

## Freshness and staleness

When inputs changed, `start` builds the fresh output but is **non-destructive to a running VM**: it does **not** replace the running VM automatically. It warns that the running VM is stale and names how to apply the new build. This preserves work and long-lived processes inside the guest (N15).

- **`--rebuild`** — evaluate, build, then **stop and replace** the running VM with the fresh one. Persistent volumes survive the restart ([`06`](./06-workspace-and-project-environment.md)).
- **`--no-rebuild`** — skip evaluation entirely and boot the last build as-is (fast path, offline). Fails clearly if the project was never built, or if the recorded store output was garbage-collected ([`11`](./11-generations-and-build-history.md)).
- **`--generation <n>`** — boot a specific retained generation instead of the current one ([`11`](./11-generations-and-build-history.md)).

`--rebuild` and `--no-rebuild` are mutually exclusive.

## Attach vs detach

By default `start` boots the VM as a **background resource and returns once the guest answers** — the VM is a persistent thing `exec`/`shell` connect to, and the wait is what lets the post-condition above hold. `--attach` instead streams the VM console until it exits; `Ctrl-C` **detaches** the console and leaves the VM running — it never stops the VM. Because an attached `start` returns when the stream ends, a guest that powers itself off leaves `--attach` returning with no VM, which is why the post-condition excludes it.

## Session boundary

"Background resource" means it outlives the **command**, not the **login**. A running VM does **not** survive the user's final logout unless the host is configured to keep that user's session manager alive across it — every VM's processes live in a scope that manager owns ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)), so its exit takes them. Enabling that is a host action, not a vivarium feature or a flag; `viv doctor`'s soft `host-linger` check reports the setting while a VM is running ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)). Decided in [`../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md`](../../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md).

A logout is therefore not a crash, and reads as one only if evidence of it survives. It does not: the runtime markers live in the session's own runtime root and go away with it ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)), so the "markers present but the process is dead" discriminator above finds nothing and the next login reports **built** — the same reading a clean `stop` leaves.

## Stopping: `viv stop`

`viv stop` drives a **running** VM (fresh or stale) to **built** through the transitional **stopping** state, escalating only as needed:

1. **Agent shutdown** — signal the in-guest agent over the control socket for an orderly guest shutdown ([`12-exec-and-shell.md`](./12-exec-and-shell.md)).
2. **Backend soft-off** — if the agent is unreachable, fall back to the backend's ACPI-class power signal.
3. **Hard poweroff** — when `--timeout` expires (default 10 s; `-1` waits indefinitely), pull the power. `--force` skips straight here (possible data loss) and conflicts with a nonzero `--timeout` — that combination is a usage error.

`stop` releases the target's host resource scope, so the VMM, every per-share filesystem daemon, and any launch helper go away together — no daemon outlives the VM it served ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)).

`stop` removes **nothing**: persistent volumes, build generations, and the project binding all survive (N18, [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md)). It is idempotent — nothing running is a no-op that exits `0`, and dead runtime files (stale pid, socket) are cleaned up on the way. A stop that cannot be confirmed even by hard poweroff reports the actual VM state and exits with the unavailable code rather than pretending success.

## Teardown: `viv destroy`

`viv destroy` is the explicit teardown — the only verb that removes data. It stops the VM (same ladder as `stop`), unlinks **all** of the project's generation GC roots, removes its persistent volumes (unless `--keep-volumes`), and deletes its runtime state. It prompts for confirmation on a TTY; non-interactive runs require `--yes`.

Beyond the vivarium-owned identity marker, `destroy` never touches the workspace, and it never touches the config root, the project binding, or store contents. The marker is the one carve-out: `destroy` removes `.vivarium/` and clears the project's identity-index entry so the next `viv start` in that directory is a clean first run ([`15-project-identity.md`](./15-project-identity.md), [`../../decisions/ADR-0043-identity-marker-lifecycle.md`](../../decisions/ADR-0043-identity-marker-lifecycle.md)). Nothing user-authored is affected — the marker is vivarium's own file and inert to the project's tooling (N9, N21). Unlinking the GC roots only makes the store paths _reclaimable_ — they are actually freed by a later `viv gc` ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)). Because the build is reproducible from the manifest, `destroy` followed by `start` costs at most a rebuild; the only irreversible loss is volume data, which is why removal is guarded by the prompt and spared by `--keep-volumes`. `destroy` is idempotent — nothing to tear down exits `0`.

## Preflight (fail-fast)

`start` runs the **hard subset** of the shared `viv doctor` probe catalog before any side effect — one probe set, reused by `doctor` (whole catalog) and each command's guard (its subset), so they never drift. The catalog — stable check ids, categories, severities, and failure codes — is owned by [`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md); the hard subset is exactly its hard-severity checks, run cheapest-and-most-fundamental first so the earliest failure is the most actionable: `nix-present` → `nix-version` → `nix-flakes-enabled` → `kvm-device-present` → `kvm-device-accessible` → `hardware-virt-available` → `host-userns-available` → `runtime-dir-usable`. The backend is not among them: it arrives in the built runner's closure rather than from the host ([`../../decisions/ADR-0049-backend-is-a-closure-member.md`](../../decisions/ADR-0049-backend-is-a-closure-member.md)).

The **admission check** (step 3 above) is a separate gate, not a catalog probe: preflight asks whether this host _can_ run a VM at all, admission asks whether it can run _another one right now_. Its thresholds and its warning are owned by [`17-resources-and-capacity.md`](./17-resources-and-capacity.md); `viv doctor` reports the same host readings as ordinary soft checks.

Each failing check reports **what / where / why / hint**, its stable check id, and a specific exit code from the program-wide sysexits taxonomy — never a generic `1`. The full legend and the per-command matrix (including `start`, `stop`, and `destroy`) are in [`14-exit-codes.md`](./14-exit-codes.md); the stream rules are [`ADR-0015`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).

## Nix validation ladder

Before paying for a full build, `start` surfaces the cheapest failure class first:

- **Parse** — pure syntax (`nix-instantiate --parse`); cheapest, local.
- **Resolve / evaluate** — flake resolution and attribute/type errors without realisation (`nix flake metadata`, a scoped `nix eval` of the VM attribute).
- **Build** — derivation realisation (`nix build`); the expensive tier, gated behind the two above.

The ladder's failure codes split by who recognized the fault. A **content defect vivarium's own rules name** — an irreconcilable merge, an equal-priority scalar tie, or a literal personal path in a shared image or piece — is `65`, and recognizing a tie means inspecting the module system's definitions before forcing the value. Anything else Nix rejects is `70`. See [`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md) and [`14-exit-codes.md`](./14-exit-codes.md).

## Output streams

`start`'s result is a side effect (a booted VM), so on success **stdout is empty**; all progress and status go to **stderr**. `stop` and `destroy` follow the same rule — empty stdout on success, one record under `--json`. The full convention is in [`ADR-0015`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).
