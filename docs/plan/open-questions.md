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

## Q-009 — Should the transient unit classify the supervisor's exit status?

[`systemd.rs`](../../src/launch/systemd.rs) sets no `Restart=`, `SuccessExitStatus=`, `RestartPreventExitStatus=`, or `RestartForceExitStatus=`, so the manager treats the supervisor's `64`, `70`, and `74` identically and the distinction survives only in the journal. Blocks: nothing shipped; it decides whether the supervisor's classification is operational or diagnostic. Raised: reviewing the supervisor's failure boundary. Exit: an ADR fixing the unit's restart posture, or a recorded decision that the codes stay diagnostic and the journal message is the operational surface.
