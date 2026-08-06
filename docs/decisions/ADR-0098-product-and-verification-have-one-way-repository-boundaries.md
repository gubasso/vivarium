# ADR-0098: Product and verification have one-way repository boundaries

## Context and Problem Statement

Nix verification code lived under `nix/`, while boot harnesses shared `scripts/` with release and repository-maintenance programs, and `nix/default.nix` imported both the measurement legs and the contract. Product source named verification source, so a probe could reach the shipped image one plausible line at a time — the drift [ADR-0095](./ADR-0095-measurement-services-live-in-a-measurement-image.md) already had to reverse once. Large shell bodies embedded in Nix strings were invisible to editors, formatters, and every pre-commit hook.

## Considered Options

- One verification root at `tests/`, holding Nix expressions beside the Rust integration tests.
- A separate `checks/` tree at the repository root for the Nix half only.
- Leave verification under `nix/` and separate it by naming convention alone.

## Decision Outcome

Chosen option: `one verification root at tests/` — Cargo ignores non-Rust files and subdirectories without `main.rs`, so a single root serves both toolchains and no new top-level directory appears. A separate `checks/` tree splits verification by toolchain rather than by domain, and a naming convention leaves the boundary unenforceable.

`nix/` contains product code. `tests/nix/` owns Nix contracts and measurement images, `tests/host/` owns executable host harnesses, and `scripts/` operates on the repository rather than the product. Verification may depend on product; product may not depend on verification.

The sole exception is `nix/flake.nix`, the publication surface that imports `tests/nix` to expose `checks`; the `?dir=nix` reference makes the repository root the source tree, so the import resolves. `scripts/check-verification-boundary` guards that textual edge and `tests/host/first-microvm-check` guards the real one, by rejecting measurement material in the shipped derivation graph.

Extensionless `scripts/foo` and `tests/host/foo` files are executable programs; a co-located `foo.sh` is a shell body Nix loads. Bodies of roughly five lines or more are extracted, taking binaries through `runtimeInputs` or a unit `path`, scalars through the environment, a required literal through `replaceVars`, and control flow as data passed to a shell loop.

## Consequences

- Good: shell sources have a lint location, Cargo excludes the verification subtrees, and product changes cannot silently acquire a probe dependency.
- Bad: the launcher JSON still carries measurement canary material into the shipped artifact. That shipping-purity gap is recorded here, not fixed; it does not reverse the dependency arrow.

## Status

Implemented

Enacted by `tests/nix/default.nix`, `nix/default.nix`'s `mkImage` seam, `scripts/check-verification-boundary`, and the coverage and boundary assertions in `tests/host/first-microvm-check`. Amends [ADR-0095](./ADR-0095-measurement-services-live-in-a-measurement-image.md), whose variant seam moved out of `nix/default.nix`.
