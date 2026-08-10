# KI-0001 investigation

## Symptom

An automatic collection inside the guest announces a byte target, deletes nothing, and reports the target met. Guest free space does not recover and the dead-path count does not fall. Reproduced on two independent boots of `tests/host/store-pressure-check --arm e`.

## Root cause

Three lines of the pinned source, in the order the collector executes them.

`src/libstore/gc.cc`, in `collectGarbage`'s `deleteFromStore` lambda:

```cpp
uint64_t bytesFreed;
deleteStorePath(realPath, bytesFreed, isKnownPath);
results.bytesFreed += bytesFreed;
if (results.bytesFreed > options.maxFreed) {
    printInfo("deleted more than %d bytes; stopping", options.maxFreed);
    throw GCLimitReached();
}
```

`src/libstore/local-overlay-store.cc`, in `LocalOverlayStore::deleteStorePath`, returns without touching `bytesFreed` when the path is absent from the upper layer — the whole body is guarded by `if (pathExists(upperPath))`. The only assignment in the chain is `deletePath`'s `bytesFreed = 0` in `src/libutil/unix/file-system.cc`, which that branch never reaches.

So an attempt on a path that exists below and not above adds an indeterminate value to `results.bytesFreed`. When it exceeds the target, the pass ends. On both measured boots the first such path ended the collection: one path attempted, nothing freed, target reported met.

## Why vivarium reaches it

The branch needs one dead path present in the lower layer and absent from the upper one, which is the ordinary state of a `local-overlay` store. Registration is not what decides it: a lower store of properly registered paths reaches the branch just as reliably, confirmed on the host below. [`../../../decisions/ADR-0038-guest-store-sharing.md`](../../../decisions/ADR-0038-guest-store-sharing.md) shares the host's literal store as the lower layer, so every entry the collector reads from the guest's store directory is such a path — it reads the directory, not only its database — and the first one ends each pass.

The limit is never absent in the guest. `LocalStore::autoGC` sets `options.maxFreed = maxFree - avail`, a finite number that the garbage value exceeds. A collection with no limit escapes, because the default `maxFreed` is `UINT64_MAX` and no accumulated garbage can exceed it. That default is also why upstream's own `tests/functional/local-overlay-store/gc.sh` builds this situation and still passes.

## Evidence

Two guest boots are recorded in the findings register under "The collection ends after one path, on an uninitialised byte count read through the overlay" in [`../../microvm-verification-harness.md`](../../microvm-verification-harness.md), with the per-iteration classification table.

Reproduced directly on the host on 2026-08-06 against Nix 2.34.8, outside vivarium: a `local-overlay` store over a registered lower store, collected with a limit a million times the size of the store, attempts one lower-layer path, deletes nothing, leaves the upper layer's own deletable garbage in place, and reports 85.7 TiB freed from a store holding roughly 1.5 MB. Nine consecutive runs behaved identically. `valgrind --track-origins=yes` names the `collectGarbage` lambda as both the reading frame and the origin of the stack allocation. So the guest boundary is not load-bearing for this defect, and the wasted-space consequence is wider than the two boots could show: garbage the collector is able to delete survives the pass.

## Recheck condition

Recheck on the next backend or Nix pin ([`../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) owns that cadence): re-run `tests/host/store-pressure-check --arm e` and read `store-pressure-collector-per-path`. The issue is resolved when a collection attempts more than one path and the dead-path count falls.

### 2026-08-10 — attempted at Nix 2.34.8, then performed

The recheck was due at the 2.34.7 to 2.34.8 pin move. The first attempt produced no verdict: `store-pressure-guest-completion` failed with the guest read as exiting `0` having never printed `COMPLETE`, and both affected checks skipped, each reporting that no collection was announced with the dead-path and free-byte fields empty. A skip is unproven, not passed, and the empty fields were the tell — nothing had measured the quantity the resolution condition asks about.

That was the harness, not this defect and not the pin. `tests/host/store-pressure-check` had never learned that launch hands off to a manager-owned transient service, so it waited on the handoff process and tore the run down seconds in; the harness Findings entry for the repair owns the cause. With the lane repaired the recheck ran the same day and reached a verdict.

### 2026-08-10 — reproduced at Nix 2.34.8

`tests/host/store-pressure-check --arm e`, on a real host, `PASS=25 FAIL=0 SKIP=0`. The resolution condition is not met and the entry stays `open` on fresh evidence rather than on age.

The collector announced its target and freed nothing measurable. Dead paths ROSE across the run, 22785 to 22811, and guest free space fell from 856,788,992 to 588,193,792 bytes rather than recovering. `store-pressure-collector-per-path` classified the attempted path as `absent_from_upper` — `00lcigchi7dpghaiczkq482gjz9ry7gg-nix-main-2.34.8`, a path present in the lower layer and absent from the upper one, which is exactly the branch the root cause above describes. One path attempted, nothing freed.

Two adjacent results from the same run, recorded because they are what the arm exists to separate this defect from. The discard chain is intact: the guest driver negotiated `VIRTIO_BLK_F_DISCARD`, `fstrim` trimmed 1 GiB of the writable layer, and the host image shrank from 4,132,900,864 to 3,330,715,648 allocated bytes. So the wasted space is the collector's, not the block layer's.
