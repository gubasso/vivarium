# 10 — VM lifecycle: `viv start`, `viv stop`, and teardown

How `viv start` takes a project from nothing to a running sandbox, how `viv stop` and `viv destroy`
bring it back down, and the rules that make each safe to run twice. The lifecycle decision and
rationale are in
[`../../decisions/ADR-0013-vm-lifecycle-and-up.md`](../../decisions/ADR-0013-vm-lifecycle-and-up.md)
(which records `start` under its former name `up`) and
[`../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md);
the output-stream and failure conventions used below are in
[`../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).

## Lifecycle states

A project's VM is in one of:

- **absent** — never built.
- **built** — a sandbox output exists in the store but no VM is running.
- **running** — a VM is up for this project.
- **stale** — a VM is running, but a layer changed so the current build's store output no longer
  matches what the running VM was booted from. Freshness is the store output path, never a separate
  digest (N4, [`04-composition-and-determinism.md`](04-composition-and-determinism.md)).

**stopping** is a transitional state passed through by `viv stop` and `viv destroy`, never a
resting state. The transitions:

```text
absent | built ──start──▶ running
running | stale ──stop──▶ (stopping) ──▶ built
running | stale | built ──destroy──▶ built (store paths linger) ──gc──▶ absent
```

`stop` can only ever reach **built** — a stopped project's build output stays pinned. A project
returns to **absent** only when `destroy` has unlinked its generations *and* a later collection has
reclaimed the store paths ([`11-generations-and-build-history.md`](11-generations-and-build-history.md)).

## What `viv start` does

`viv start` drives the project toward **running**, in this order, failing closed at the first unmet
step (see *Preflight* below):

1. **Resolve** the bound manifest by the precedence in
   [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md). None resolves → fail closed (N7).
2. **Preflight** the host — refuse before any build or launch if a hard prerequisite is missing.
3. **Evaluate and build** the outer flake to a store output. Identical inputs reuse the cached
   output (N4); the result is recorded as a **generation**
   ([`11-generations-and-build-history.md`](11-generations-and-build-history.md)).
4. **Ensure running** — boot the VM if one is not already up for this project, injecting the
   workspace host path at launch (N5) and mounting it read-write at the fixed guest path
   ([`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md)).
   `exec`/`shell` add the control-socket liveness handshake specified in
   [`12-exec-and-shell.md`](12-exec-and-shell.md).

`start` is **idempotent**: on a fresh, already-running VM it is a no-op that exits `0` (N15). This
"ensure running" step is the shared routine `viv exec` and `viv shell` reuse when they start the VM
if needed.

`exec` and `shell` call the same ensure-running routine. When the control-socket ping proves the VM
is already running for the same project, they skip preflight/build/boot and attach a session. When
they must cold-start, they run the same hard preflight subset as `start` before any build or launch.

## Freshness and staleness

When inputs changed, `start` builds the fresh output but is **non-destructive to a running VM**: it does
**not** replace the running VM automatically. It warns that the running VM is stale and names how to
apply the new build. This preserves work and long-lived processes inside the guest (N15).

- **`--rebuild`** — evaluate, build, then **stop and replace** the running VM with the fresh one.
  Persistent volumes survive the restart ([`06`](06-workspace-and-project-environment.md)).
- **`--no-rebuild`** — skip evaluation entirely and boot the last build as-is (fast path, offline).
  Fails clearly if the project was never built, or if the recorded store output was garbage-collected
  ([`11`](11-generations-and-build-history.md)).
- **`--generation <n>`** — boot a specific retained generation instead of the current one
  ([`11`](11-generations-and-build-history.md)).

`--rebuild` and `--no-rebuild` are mutually exclusive.

## Attach vs detach

By default `start` boots the VM as a **background resource and returns** — the VM is a persistent
thing `exec`/`shell` connect to. `--attach` instead streams the VM console until it exits; `Ctrl-C`
**detaches** the console and leaves the VM running — it never stops the VM.

## Stopping: `viv stop`

`viv stop` drives a **running** (or **stale**) VM to **built** through the transitional
**stopping** state, escalating only as needed:

1. **Agent shutdown** — signal the in-guest agent over the control socket for an orderly guest
   shutdown ([`12-exec-and-shell.md`](12-exec-and-shell.md)).
2. **Backend soft-off** — if the agent is unreachable, fall back to the backend's ACPI-class power
   signal.
3. **Hard poweroff** — when `--timeout` expires (default 10 s; `-1` waits indefinitely), pull the
   power. `--force` skips straight here (possible data loss) and conflicts with a nonzero
   `--timeout` — that combination is a usage error.

`stop` removes **nothing**: persistent volumes, build generations, and the project binding all
survive (N18, [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md)).
It is idempotent — nothing running is a no-op that exits `0`, and dead runtime files (stale pid,
socket) are cleaned up on the way. A stop that cannot be confirmed even by hard poweroff reports
the actual VM state and exits with the unavailable code rather than pretending success.

## Teardown: `viv destroy`

`viv destroy` is the explicit teardown — the only verb that removes data. It stops the VM (same
ladder as `stop`), unlinks **all** of the project's generation GC roots, removes its persistent
volumes (unless `--keep-volumes`), and deletes its runtime state. It prompts for confirmation on a
TTY; non-interactive runs require `--yes`.

`destroy` never touches the workspace, the config root, the project binding, or store contents:
unlinking the GC roots only makes the store paths *reclaimable* — they are actually freed by a
later `viv gc` ([`11-generations-and-build-history.md`](11-generations-and-build-history.md)).
Because the build is reproducible from the manifest, `destroy` followed by `start` costs at most a
rebuild; the only irreversible loss is volume data, which is why removal is guarded by the prompt
and spared by `--keep-volumes`. `destroy` is idempotent — nothing to tear down exits `0`.

## Preflight (fail-fast)

`start` runs the **hard subset** of the shared `viv doctor` probe catalog before any side effect — one
probe set, reused by `doctor` (whole catalog) and each command's guard (its subset), so they never
drift. The catalog — stable check ids, categories, severities, and failure codes — is owned by
[`13-doctor-and-health-checks.md`](13-doctor-and-health-checks.md); the hard subset is exactly its
hard-severity checks, run cheapest-and-most-fundamental first so the earliest failure is the most
actionable: `nix-present` → `nix-version` → `nix-flakes-enabled` → `kvm-device-present` →
`kvm-device-accessible` → `hardware-virt-available` → `backend-binary-present`.

Each failing check reports **what / where / why / hint**, its stable check id, and a specific exit
code from the program-wide sysexits taxonomy — never a generic `1`. The full legend and the
per-command matrix (including `start`, `stop`, and `destroy`) are in
[`14-exit-codes.md`](14-exit-codes.md); the stream rules are
[`ADR-0015`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).

## Nix validation ladder

Before paying for a full build, `start` surfaces the cheapest failure class first:

- **Parse** — pure syntax (`nix-instantiate --parse`); cheapest, local.
- **Resolve / evaluate** — flake resolution and attribute/type errors without realisation
  (`nix flake metadata`, a scoped `nix eval` of the VM attribute).
- **Build** — derivation realisation (`nix build`); the expensive tier, gated behind the two above.

## Output streams

`start`'s result is a side effect (a booted VM), so on success **stdout is empty**; all progress and
status go to **stderr**. `stop` and `destroy` follow the same rule — empty stdout on success, one
record under `--json`. The full convention is in
[`ADR-0015`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).
