# ADR-0076: Test lanes and what each proves

## Context and Problem Statement

The virtualization-gated acceptance lane exists, but nothing says how the rest of the suite is organized or which lane proves which invariant. Coverage then drifts toward whatever is easiest to write, and CI cannot tell a required signal from an optional one.

## Considered Options

- One suite, gated test by test
- Named lanes, each invariant proved by the cheapest lane that can
- Snapshot everything and rely on review

## Decision Outcome

Chosen option: named lanes, one runner profile each.

Six lanes. All but the fourth and fifth run with no Nix, no network, and no virtualization, asserting on a rendered artifact; those two need Nix, nothing more.

1. Unit — parse, closed-key rejection, name resolution, precedence, XDG placement, secret-path validation. Property tests for two things only: key-table round-trip, and precedence being total and probe-order independent.
2. Structured golden — the generated flake tree, `--json` records, `config eval`, with absolute paths, project ids, store hashes, and timestamps filtered out. CI fails on a missing or changed snapshot rather than writing one.
3. Text-contract golden — usage output and the five-slot error skeleton.
4. Evaluation — that the generated flake evaluates, without building it. Runs at the existing Nix-present, virtualization-absent gate rung, which nothing uses today; a byte-identical snapshot proves no such thing.
5. Purity and 6. Non-invasion — ADR-0077.

Both egress modes are covered twice: once as a golden, once end-to-end.

Lanes 1–3 and 6 are required on every change; lanes 4 and 5 wherever Nix is present, including CI. The gated lane stays informational until a virtualization-capable runner exists, and a skipped trial is never implementation evidence.

Every golden file names the spec line it encodes, so accepting a diff is a spec decision — otherwise a snapshot nobody traced becomes the contract.

## Consequences

- Good: most of the suite runs anywhere, so a contributor without virtualization gets real signal.
- Good: each invariant has one named home.
- Bad: some scenarios are covered twice, once planned and once end-to-end.
- Bad: each lane needs an ungated trial, or an all-ignored run is itself a failure.

## Status

Accepted

Specified in [`../reference/testing-lanes.md`](../reference/testing-lanes.md).

Amended by [`./ADR-0114-a-lane-runs-where-a-place-names-it.md`](./ADR-0114-a-lane-runs-where-a-place-names-it.md) — a runner profile is now one per executable lane, selected by binary name, rather than one per proof lane; the gated acceptance lane gates a capable host rather than being informational; and the ungated-trial-per-lane consequence no longer applies, because nothing skips. The six proof lanes and what each proves stand.
