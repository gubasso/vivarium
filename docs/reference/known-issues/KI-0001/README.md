# KI-0001 — a local-overlay collection ends on an uninitialised byte count

- Status: `open`
- Severity: high — space-triggered collection does not bound the guest store volume.
- External system: Nix 2.34.7 (`nix-main`), pinned by [`nix/flake.lock`](../../../../nix/flake.lock).
- Affected checks: `store-pressure-collector-freed` and `store-pressure-collector-per-path` in [`../../../../tests/host/store-pressure-check`](../../../../tests/host/store-pressure-check).
- Mask: none. Nothing in vivarium suppresses or works around this, and no check expects the failure.

An automatic collection inside the guest announces a byte target, deletes nothing, and reports the target met. Guest free space does not recover and the dead-path count does not fall.

- [`issue.yaml`](./issue.yaml) — supplies the machine-readable case metadata.
- [`investigation.md`](./investigation.md) — owns the symptom, root cause, vivarium path, and recheck condition.
- [`microvm-verification-harness.md`](../../microvm-verification-harness.md) — owns the durable guest evidence and per-iteration classification table.
- [`escalation.md`](./escalation.md) — records the public upstream report, reproducer, Valgrind trace, and verified fix.
- [`investigation.md#recheck-condition`](./investigation.md#recheck-condition) — defines the exact command and resolution signal.
