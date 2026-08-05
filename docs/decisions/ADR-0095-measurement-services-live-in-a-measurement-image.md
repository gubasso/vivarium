# ADR-0095: Measurement services live in a measurement image, never in the base image

## Context and Problem Statement

Four probe units and two upstream Nix test hooks sat in the guest module every user would get. They answer questions that are now answered, and the free-space hook is worse than an idle unit: it is a behaviour switch on the shipped store daemon, reachable by anything that can write `/run/vivarium/nix-free-space`. Separately, the one measurement still open needs a guest whose collection thresholds are scaled, and those are readable by the daemon only when built into the image.

## Considered Options

- Leave the probes in the base image and gate them at run time on an instruction file.
- Edit the guest module temporarily for each measurement, boot, and revert.
- Give the build a variant seam, and move every probe behind it.

## Decision Outcome

Chosen option: a variant seam — `nix/default.nix` takes a `variant` attrset, and `nix/measurement/` composes probe legs that reach an image only through it. The shipped image selects none.

Run-time gating is what this record rejects: inert is not absent, an inert unit still ships an `ExecStopPost` and a console writer, and the hook is not inert in any useful sense. A temporary edit cannot be reviewed or reproduced.

Composition, not the leg bodies, owns ordering and stopping. Legs used to name each other in `after`/`before`, and systemd treats an edge to an absent unit as a silent no-op — so an image built without one composed cleanly and ran a different topology without saying so.

## Consequences

- Good: the shipped image contains exactly one vivarium unit, asserted at build time as an allowlist rather than a denylist, so a probe added tomorrow fails the check too.
- Good: a measurement is now a reviewable artifact rather than an uncommitted edit.
- Bad: the shipped image can no longer stop itself, so the harness must stop it the way a user would — over the API socket.
- Bad: one more flake attribute per measurement shape, and lanes must build the right one.

## Status

Implemented

Enacted by `nix/default.nix`'s `variant` argument, `nix/measurement/`, `nix/store-layout.nix` and `nix/contract.nix`; exposed as `packages.first-microvm-{measurement,scaled,bench-threads-N}` in `nix/flake.nix`.

The relocation is pure, and that is what licensed not re-running three closed lanes — verified 2026-08-05. The three legs that produced the existing register entries resolve to the same script derivation before and after the move, their `After=` sets are identical, and the tmpfiles rules are an identical set. `scripts/first-microvm-check` against the measurement image returned `PASS=33 FAIL=0 SKIP=1`, matching the pre-refactor cold-boot result. Registered in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

The shipped image's stop path is measured, and it was an unverified assumption before this — 2026-08-05. `ch-remote power-button` over the API socket reached the guest, which ran its full shutdown transaction and powered off in 2.0 s. Nothing in the guest module configures ACPI handling or `logind` policy, so this had rested on kernel and `systemd-logind` defaults that no boot had confirmed.

The microVM's outputs live in their own flake, and the repository root's is untouched by them — 2026-08-05. The root `flake.nix` is the development environment: the Rust toolchain, the pre-commit hook runtimes, the devShell direnv activates. It is not part of what vivarium builds, and this record's variants would have put seven product attributes and their rationale into it. They live at `nix/flake.nix` with their own lock instead, which also decouples the guest's nixpkgs from the one the Rust toolchain wants — two pins on two clocks, which is what [`ADR-0078`](./ADR-0078-backend-advisory-response-is-a-released-pin-move.md) already says about backend pins. Verified inert: the base guest system's derivation path is byte-identical across the move.

Amends [`ADR-0048`](./ADR-0048-guest-module-only-vivarium-owns-the-runner.md) — the flake now exposes several runner attributes rather than one. The rule is unchanged: every one of them is vivarium's own construction, and no upstream runner derivation is forced. The negative-reference check still passes against the shipped attribute.
