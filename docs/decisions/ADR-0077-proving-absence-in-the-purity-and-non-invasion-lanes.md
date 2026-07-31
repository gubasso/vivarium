# ADR-0077: Proving absence in the purity and non-invasion lanes

## Context and Problem Statement

N5 and N19 say no host path and no launch-channel value reaches a build input; N9 says no user file is touched. Both are absence claims, and a test that scans for suspicious-looking strings cannot prove one — it is the heuristic ADR-0069 already refused for redaction.

## Considered Options

- Deny-list scanning of build inputs for host-looking paths and secret-looking values
- A unique per-run canary plus a metamorphic equality over the derivation graph
- Kernel-enforced write confinement for non-invasion

## Decision Outcome

Chosen option: **canary plus metamorphic equality**, and a tree comparison for non-invasion.

**Purity.** Each run plants a unique random token in the workspace host path and in every launch-channel value, then asserts it appears nowhere in the recursive derivation graph — its input sources, input derivations, and environment. A unique token has no false negatives, where a deny-list of host-looking paths has an unbounded tail. Separately, the same manifest is built from two different host paths, and again with only launch-channel data changed; the derivation must be identical either way. That states N3, N5, and N19 as an equality, stronger than any scan.

**Non-invasion.** Every command runs against a fixture project whose complete tree and version-control status are compared before and after. The only permitted difference is the `.vivarium/` marker, N9's sole exception; a read-only command must show none. The comparison is automatic in the fixture, because an opt-in invariant check is the one a new trial forgets.

Kernel-enforced confinement was rejected: it proves more, but needs privileges many hosts lack, turning an assertion into a skip — and an observation of the execution environment must never become a fact about a host.

## Consequences

- Good: both lanes fail loudly on any host, needing no privileges and no virtualization.
- Good: the equality catches a leak the canary never sees, needing no guess about what leaked.
- Bad: the purity lane reads a derivation-inspection format upstream calls experimental, so it may need revising.
- Bad: a tree comparison detects a write after it happened rather than preventing it.

## Status

Accepted

Specified in [`../reference/testing-lanes.md`](../reference/testing-lanes.md). Lane membership is decided in [`ADR-0076`](./ADR-0076-test-lanes-and-what-each-proves.md).
