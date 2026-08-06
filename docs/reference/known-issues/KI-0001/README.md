# KI-0001 — a local-overlay collection ends on an uninitialised byte count

- Status: `open`
- Severity: high — space-triggered collection does not bound the guest store volume.
- External system: Nix 2.34.7 (`nix-main`), pinned by [`nix/flake.lock`](../../../../nix/flake.lock).
- Affected checks: `store-pressure-collector-freed` and `store-pressure-collector-per-path` in [`../../../../tests/host/store-pressure-check`](../../../../tests/host/store-pressure-check).
- Mask: none. Nothing in vivarium suppresses or works around this, and no check expects the failure.

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

## Upstream reference

[NixOS/nix#16269](https://github.com/NixOS/nix/issues/16269), filed 2026-08-06. It carries the host reproducer, the `valgrind` origin trace, and a one-line fix at the call site verified against a patched 2.34.8 build.

## Revert condition

Recheck on the next backend or Nix pin ([`../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) owns that cadence): re-run `tests/host/store-pressure-check --arm e` and read `store-pressure-collector-per-path`. The issue is resolved when a collection attempts more than one path and the dead-path count falls.
