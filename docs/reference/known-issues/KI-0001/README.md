# KI-0001 — a local-overlay collection ends on an uninitialised byte count

- Status: `open`
- Severity: high — space-triggered collection does not bound the guest store volume.
- External system: Nix 2.34.7 (`nix-main`), pinned by [`nix/flake.lock`](../../../../nix/flake.lock).
- Affected checks: `store-pressure-collector-freed` and `store-pressure-collector-per-path` in [`../../../../scripts/store-pressure-check`](../../../../scripts/store-pressure-check).
- Mask: none. Nothing in vivarium suppresses or works around this, and no check expects the failure.

## Symptom

An automatic collection inside the guest announces a byte target, deletes nothing, and reports the target met. Guest free space does not recover and the dead-path count does not fall. Reproduced on two independent boots of `scripts/store-pressure-check --arm e`.

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

The branch needs a `local-overlay` store whose lower layer physically holds paths its database does not know. [`../../../decisions/ADR-0038-guest-store-sharing.md`](../../../decisions/ADR-0038-guest-store-sharing.md) shares the host's literal store as the lower layer, so the guest's store directory is full of them and the collector treats each as garbage — it reads the directory, not only its database. A store whose lower layer is a generated image of registered paths never takes this branch.

## Evidence

Recorded in the findings register under "The collection ends after one path, on an uninitialised byte count read through the overlay" in [`../../microvm-verification-harness.md`](../../microvm-verification-harness.md), with the per-iteration classification table.

## Upstream reference

Not yet reported.

## Revert condition

Recheck on the next backend or Nix pin ([`../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) owns that cadence): re-run `scripts/store-pressure-check --arm e` and read `store-pressure-collector-per-path`. The issue is resolved when a collection attempts more than one path and the dead-path count falls.
