# Open questions

This is triage, not a queue. Every question blocks named work and leaves through exactly one recorded exit.

## Q-005 — Which Rust networking crates satisfy the namespace, tap, nftables, and gating-resolver boundaries?

Blocks: slice 004, `In scope`. Raised: ADR-0064's deferred crate selection. Exit: a slice 004 `Revisions` line recording the selected seam and evidence.

## Q-006 — Can a user-space vhost-user uplink drive the pinned Cloud Hypervisor directly?

Blocks: slice 004 topology choice, not its egress contract. Raised: ADR-0064. Exit: a recorded experiment; failure uses the pre-authorized tap-plus-uplink fallback.

## Q-007 — Is the pinned Nix derivation JSON stable enough for the purity lane?

Blocks: slice 007 purity acceptance. Raised: ADR-0077's experimental-interface caveat. Exit: a slice 007 `Revisions` line after revalidation, either retaining the interface or naming the replacement.

## Q-008 — Does `start` return `74`, or does its I/O failure belong to another category?

The [`start` row](../reference/spec/14-exit-codes.md) does not admit `74`, and the slice-002 handoff in [`src/main.rs`](../../src/main.rs) returns it for its runtime-directory and socket failures. One of the two is wrong. The discrepancy is now legible rather than scattered: [`ExitKind`](../../src/exit.rs) holds the taxonomy and `StartError::exit_code` chooses from it in one exhaustive match, so the answer changes one arm. Blocks: slice 005, which realizes the rest of ADR-0033 across the command surface. Raised: reviewing the supervisor's failure boundary. Exit: either a `14-exit-codes.md` matrix amendment admitting `74` for `start`, or a slice 005 `Revisions` line recording the category those failures move to.

## Q-010 — How does the supervisor publish the descriptor limit the contract asserts?

The limit is spelled six times across a boundary Nix cannot cross: `524_288` in [`spec.rs`](../../src/launch/spec.rs), and `524288` in [`policy.rs`](../../src/launch/policy.rs), [`systemd.rs`](../../src/launch/systemd.rs), [`launch-arguments.nix`](../../nix/launch-arguments.nix), [`contract.sh`](../../tests/nix/contract.sh), and [`first-microvm-check`](../../tests/host/first-microvm-check). Nothing derives one from another, so the contract's assertion is a literal restated rather than two independently realised artifacts compared — the defect that file's own header exists to avoid. Blocks: eliminating the last literal-only cross-boundary assertion. Raised: ADR-0098's deferred gap. Exit: a supervisor introspection subcommand that emits its constants, which the contract then diffs against the launcher JSON.

## Q-011 — Why do four host runbooks fail on this host, in two clusters, independently of every version we can move?

The 2026-08-10 sweep re-ran every host runbook after the backend pin moved. `store-density-check` and `guest-agent-check` pass. The other four fail, in two distinguishable clusters, and one symptom spans both.

The reporting cluster: `store-gc-interlock-check`, `store-pressure-check` and `share-benchmark-check` boot a guest that exits `0` having never printed the diagnostic marker their checks read, leaving a zero-byte `console.log`. `share-benchmark-check` adds its own variant, with pools 1, 2 and 4 failing to start where pool 0 starts. This is not every guest: `first-microvm-check`'s measurement guest completes and its console captures over a megabyte, and `guest-agent-check`'s guest answers on two vsock ports.

The posture cluster is `first-microvm-check`'s alone: `IOWeight` is unset on the transient unit where the check requires it or a memory bound; the confinement check sees two processes per virtiofsd role rather than one, the second retaining a full effective capability set, and finds no `--landlock` in the cloud-hypervisor argv on a host that reports Landlock — though the launch spec does carry `landlock_enable`. Whether that last one is a defect or a check reading argv for a fact that now lives in the API JSON is exactly what is unresolved.

Spanning both: all four retain their runtime directory contents after shutdown, against an allowlisted cleanup that is supposed to empty it.

Three candidate causes are eliminated by measurement rather than argument. Not the backend pin: reverting `nix/flake.lock` alone reproduces the failures identically at cloud-hypervisor 52.0 and Nix 2.34.7, same checks and same counts. Not slice 003: a clean worktree at `a81140d`, the commit before the guest-agent work, fails `store-pressure-check` harder, not launching at all. Not any version delta, on the same evidence. What remains is this host's configuration or a defect older than both, and those two are not yet separated — no second target host has been tried, which is the obvious next measurement.

Blocks: [KI-0001](../reference/known-issues/KI-0001/README.md)'s recheck, due at Nix 2.34.8 and unperformable while no collection is ever announced; and any figure in the [verification harness](../reference/microvm-verification-harness.md) that a pin move would otherwise refresh. Blocks [slice 010](./slices/010-repair-the-host-runbooks/README.md), whose first ordered item is this question and whose remaining scope depends on the answer. Raised: the 2026-08-10 sweep closing slice 003. Exit: a recorded run on a second capable host, which either reproduces the failures — making them a vivarium defect owned by slice 010 — or does not, making them a property of this host recorded in the harness.
