# Open questions

This is triage, not a queue. Every question blocks named work and leaves through exactly one recorded exit.

## Q-005 — Which Rust networking crates satisfy the namespace, tap, nftables, and gating-resolver boundaries?

Blocks: slice 004, `In scope`. Raised: ADR-0064's deferred crate selection. Exit: a slice 004 `Revisions` line recording the selected seam and evidence.

## Q-006 — Can a user-space vhost-user uplink drive the pinned Cloud Hypervisor directly?

Blocks: slice 004 topology choice, not its egress contract. Raised: ADR-0064. Exit: a recorded experiment; failure uses the pre-authorized tap-plus-uplink fallback.

## Q-007 — Is the pinned Nix derivation JSON stable enough for the purity lane?

Blocks: slice 007 purity acceptance. Raised: ADR-0077's experimental-interface caveat. Exit: a slice 007 `Revisions` line after revalidation, either retaining the interface or naming the replacement.

## Q-008 — Does `start` return `74`, or does its I/O failure belong to another category?

The [`start` row](../reference/spec/14-exit-codes.md) does not admit `74`, and the slice-002 handoff in [`src/main.rs`](../../src/main.rs) returns it for its runtime-directory and socket failures. One of the two is wrong. The discrepancy is now legible rather than scattered: [`ExitKind`](../../src/exit.rs) holds the taxonomy and `StartError::exit_code` chooses from it in one exhaustive match, so the answer changes one arm. Blocks: slice 012, which is where `start` stops being a designed command and returns a code to a user. Raised: reviewing the supervisor's failure boundary. Exit: either a `14-exit-codes.md` matrix amendment admitting `74` for `start`, or a slice 012 `Revisions` line recording the category those failures move to.

## Q-010 — How does the supervisor publish the descriptor limit the contract asserts?

The limit is spelled six times across a boundary Nix cannot cross: `524_288` in [`spec.rs`](../../src/launch/spec.rs), and `524288` in [`policy.rs`](../../src/launch/policy.rs), [`systemd.rs`](../../src/launch/systemd.rs), [`launch-arguments.nix`](../../nix/launch-arguments.nix), [`contract.sh`](../../tests/nix/contract.sh), and [`first-microvm-check`](../../tests/host/first-microvm-check). Nothing derives one from another, so the contract's assertion is a literal restated rather than two independently realised artifacts compared — the defect that file's own header exists to avoid. Blocks: eliminating the last literal-only cross-boundary assertion. Raised: ADR-0098's deferred gap. Exit: a supervisor introspection subcommand that emits its constants, which the contract then diffs against the launcher JSON.

## Q-011 — Do `start` and `config eval` admit owned generated-tree and lock I/O as `74`?

The [`14-exit-codes.md` legend](../reference/spec/14-exit-codes.md) classifies genuine I/O failure on a vivarium-owned cache or data channel as `74`, but the `start` and `config eval` rows do not admit that code. Item 3 of slice 011 now has typed `74` failures for generated-tree publication and lock staging or persistence, so the CLI adapter cannot hide the mismatch behind `70`. Blocks: slice 011 item 5's CLI adapter and item 6's unskipping. Raised: implementing generated-flake materialization and pin staging. Exit: amend the matrix to admit `74` for these item-3 channels, or record the governing category change under slice 011 `Revisions` before wiring either command.
