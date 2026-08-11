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

## Q-009 — Does the generated flake carry a baseline pin for its own inputs?

Its `nixpkgs` and `microvm` inputs name branches, so the first evaluation of every target resolves them from upstream `HEAD`, and two projects bound a day apart silently get different revisions — an input jump nobody announced (N3). The development half of this is settled and is no longer what the question asks: every lane pins the baseline to the store paths this repository's own [`nix/flake.lock`](../../nix/flake.lock) resolves to, through the seam [`BaselineInputs`](../../src/config/flake.rs) reads, so the suite makes no upstream request at all. What stays open is the shipped default: whether a released vivarium should also carry a pin, which would remove the drift and create an obligation to move that pin. Blocks: nothing in flight. Raised: landing item 5, where branch resolution exhausted the anonymous GitHub rate limit and closed the `ConfigEval` gate. Exit: either a rendered baseline pin with a named update path, or a recorded decision that a released tool resolves live and the drift is the user's to pin through their lock.

## Q-010 — How does the supervisor publish the descriptor limit the contract asserts?

The limit is spelled six times across a boundary Nix cannot cross: `524_288` in [`spec.rs`](../../src/launch/spec.rs), and `524288` in [`policy.rs`](../../src/launch/policy.rs), [`systemd.rs`](../../src/launch/systemd.rs), [`launch-arguments.nix`](../../nix/launch-arguments.nix), [`contract.sh`](../../tests/nix/contract.sh), and [`first-microvm-check`](../../tests/host/first-microvm-check). Nothing derives one from another, so the contract's assertion is a literal restated rather than two independently realised artifacts compared — the defect that file's own header exists to avoid. Blocks: eliminating the last literal-only cross-boundary assertion. Raised: ADR-0098's deferred gap. Exit: a supervisor introspection subcommand that emits its constants, which the contract then diffs against the launcher JSON.

## Q-012 — Which command enforces N24, and can it be enforced before launch at all?

Nothing checks it. [`../../src/config/merged.rs`](../../src/config/merged.rs) classifies the N11 literal-path half of a shared layer's mount sources, and the session-directory half — a `source` resolving to `/tmp`, `/var/tmp`, `${XDG_RUNTIME_DIR}`, or an ancestor of one — has no classifier anywhere. Unlike N11 it may not be decidable from artifact text: the sources it forbids are named by a host variable that resolves at launch, so a merged-configuration check could only catch the literal spellings. Blocks: nothing today, because no launch path consumes a mount yet; slice 012's mount construction is where it stops being theoretical. Raised: reviewing item 5. Exit: either a classifier in the merged view for the decidable spellings plus a launch-time check for the rest, or a spec line placing the whole invariant at launch.

## Q-013 — Can `config sources` report an irreconcilable merge?

[`01-command-surface.md`](../reference/spec/01-command-surface.md) fixes `conflicts` at two kinds, `tie` and `literal-path`, so a duplicate volume name — spec/14's own cited instance of an irreconcilable merge — has nowhere machine-readable to go. `viv config eval` refuses it with `65` as specified, and the provenance view a user reaches for next reports nothing about it. Widening the vocabulary is the obvious fix and is exactly the kind of change the two-field entry was kept narrow to avoid. Blocks: nothing in flight; it is a gap in the pair's division of labour rather than a defect in either half. Raised: reviewing item 5. Exit: either a third `kind` with the trial that reads it, or a spec line saying an irreconcilable merge is `config eval`'s to report alone.
