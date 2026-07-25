# 10 — VM lifecycle and `viv up`

How `viv up` takes a project from nothing to a running sandbox, and the rules that make it safe to
run twice. The decision and rationale are in
[`../../decisions/ADR-0013-vm-lifecycle-and-up.md`](../../decisions/ADR-0013-vm-lifecycle-and-up.md);
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

## What `viv up` does

`viv up` drives the project toward **running**, in this order, failing closed at the first unmet
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

`up` is **idempotent**: on a fresh, already-running VM it is a no-op that exits `0` (N15). This
"ensure running" step is the shared routine `viv exec` and `viv shell` reuse when they start the VM
if needed.

## Freshness and staleness

When inputs changed, `up` builds the fresh output but is **non-destructive to a running VM**: it does
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

By default `up` boots the VM as a **background resource and returns** — the VM is a persistent thing
`exec`/`shell` connect to. `--attach` instead streams the VM console until it exits; `Ctrl-C`
**detaches** the console and leaves the VM running — it never stops the VM.

## Preflight (fail-fast)

`up` runs the **hard subset** of the shared `viv doctor` probe catalog before any side effect — one
probe set, reused by `doctor` (whole catalog) and each command's guard (its subset), so they never
drift. Checks run cheapest-and-most-fundamental first, so the earliest failure is the most
actionable:

1. `nix` present → `nix` meets the minimum version.
2. Flakes enabled — `experimental-features` includes `nix-command flakes`.
3. `/dev/kvm` present → `/dev/kvm` accessible to the user (split so remediation differs: enable
   virtualization vs. join the `kvm` group).
4. Hardware virtualization available.
5. The selected backend binary is present.

Each failing check reports **what / where / why / hint**, a stable check id, and a specific exit code
from the sysexits taxonomy (for example `69` unavailable, `77` permission, `78` config) — never a
generic `1`. Exit codes and stream rules are specified in
[`ADR-0015`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).

## Nix validation ladder

Before paying for a full build, `up` surfaces the cheapest failure class first:

- **Parse** — pure syntax (`nix-instantiate --parse`); cheapest, local.
- **Resolve / evaluate** — flake resolution and attribute/type errors without realisation
  (`nix flake metadata`, a scoped `nix eval` of the VM attribute).
- **Build** — derivation realisation (`nix build`); the expensive tier, gated behind the two above.

## Output streams

`up`'s result is a side effect (a booted VM), so on success **stdout is empty**; all progress and
status go to **stderr**. Under `--json`, stdout carries one machine-readable record and nothing else.
The full convention is in [`ADR-0015`](../../decisions/ADR-0015-cli-output-and-failure-contract.md).
