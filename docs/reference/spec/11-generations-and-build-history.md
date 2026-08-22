# 11 — Generations and build history

vivarium keeps a numbered history of a project's built sandboxes, so a past build can be booted or rolled back to. The decision and rationale are in [`../../decisions/ADR-0014-build-generations-and-gc-roots.md`](../../decisions/ADR-0014-build-generations-and-gc-roots.md).

## What a generation is

A generation is a retained, numbered build output for a project's manifest. Each successful `viv start` build appends the next generation. Because a build's freshness key is its store output path (N4, [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)), a generation is exactly a pinned pointer to one such output plus the metadata needed to identify it.

## Mechanism: a per-project Nix profile

Generations are held in a per-project Nix profile under the state root. A Nix profile is a chain of numbered symlinks, and each generation's symlink is a garbage-collector root — so retained builds survive `nix-collect-garbage` without any extra bookkeeping (N14, [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)). Using a profile (rather than ad-hoc symlinks) also yields listing and rollback semantics directly.

## On-disk layout

Under the state root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)):

```text
$XDG_STATE_HOME/vivarium/projects/<manifest>/<target>/
  current            -> generations/<n>
  generations/<n>    -> /nix/store/…-vivarium-vm   # GC root; survives garbage collection
  metadata/<n>.json                                 # store path, lock digest, manifest, backend, built_at
  metadata/<n>.lock                                 # the lockfile this generation was built against
```

`<manifest>` is the selected manifest name and therefore the sandbox key; `<target>` is the reserved component defined in [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md).

Each generation retains the whole lockfile it was built against, not merely a revision. A revision names where one input pointed; reproducing an evaluation needs the pinned graph, and the live lock under the data root has moved on by then ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)). What is retained is the lock that was in force — a team's override lock when one was present, otherwise the per-target lock — so a generation reproduces even after the team's pin is withdrawn. The `lock_digest` in the metadata is the content digest of that snapshot, which is what `viv generations list` reports and what makes two generations comparable at a glance.

## Commands

All generation management lives under the `viv generations` family ([`../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](../../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md)):

- `viv start --no-rebuild` — boot the current generation without re-evaluating ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)).
- `viv start --generation <n>` — boot a specific retained generation.
- `viv generations list` — list generations for the project: number, timestamp, and store path.
- `viv generations activate` / `rollback` — move the `current` pointer to another retained generation.
- `viv generations prune` — unlink old generations' GC roots under a retention policy (`--keep <n>` or `--older-than <dur>`) so a later collection can reclaim the store space. A pruned generation's metadata and lock snapshot go with it. It refuses to unlink the generation a running VM was booted from and exits `75` (stop first), the same guard `viv volume rm` uses: that generation's root is what keeps the running guest's own closure out of a collection's reach, and unlinking it under a live VM leaves the store share's overlay undefined ([`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md), [`../../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md`](../../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md)). The guard reads the boot record ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)), never `current` — `--generation <n>` and a stale running VM both name a generation `current` has moved past.
- `viv gc` — run the store garbage collector. This is a global, whole-store sweep: it reclaims every store path unreachable from any GC root on the machine, vivarium's or not, and never removes anything still pinned. Unlinking (`generations prune`, `viv destroy`) and reclaiming (`viv gc`) are deliberately separate steps.

Exit codes for the `generations` family and `viv gc` follow the per-command matrix in [`14-exit-codes.md`](./14-exit-codes.md).

## Normative notes

- A build with only a bare `result` / `--out-link` symlink is not durable history: such links are easy to delete and clutter the project tree. Generations are held as GC roots under the state root instead, never in the project's own directory (N9).
- Generation numbers are monotonic and never reused; deleting a generation leaves a gap. Do not assume dense numbering.
- Pruning must unlink the GC root first, then let Nix reclaim the store path on a later collection.
- `--no-rebuild` and `--generation <n>` must fail clearly if the recorded store output was garbage-collected — a missing pin is a legible error, not a silent rebuild.
