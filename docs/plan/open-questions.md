# Open questions

This is triage, not a queue. Every question blocks named work and leaves through exactly one recorded exit.

## Q-004 — Which supervision model keeps every child and the console reader inside the VM lifetime?

Blocks: slice 002, `Core`, and its detached-start acceptance assertion. Raised: promotion of the Stage 3 supervision and console work. Exit: an ADR opened within slice 002.

## Q-005 — Which Rust networking crates satisfy the namespace, tap, nftables, and gating-resolver boundaries?

Blocks: slice 004, `In scope`. Raised: ADR-0064's deferred crate selection. Exit: a slice 004 `Revisions` line recording the selected seam and evidence.

## Q-006 — Can a user-space vhost-user uplink drive the pinned Cloud Hypervisor directly?

Blocks: slice 004 topology choice, not its egress contract. Raised: ADR-0064. Exit: a recorded experiment; failure uses the pre-authorized tap-plus-uplink fallback.

## Q-007 — Is the pinned Nix derivation JSON stable enough for the purity lane?

Blocks: slice 007 purity acceptance. Raised: ADR-0077's experimental-interface caveat. Exit: a slice 007 `Revisions` line after revalidation, either retaining the interface or naming the replacement.
