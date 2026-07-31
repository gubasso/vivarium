# ADR-0007: Default to open egress with an opt-in allowlist

## Context and Problem Statement

Sandboxes routinely run AI coding agents that must fetch packages, clone repositories, and research freely. A restrictive default network breaks those workflows and generates constant friction. At the same time, some workloads want a locked-down network. The default must serve the common case without removing the ability to restrict.

## Considered Options

- **Default-deny with an allowlist** — no egress unless a host is explicitly permitted.
- **Default-open with an opt-in allowlist** — unrestricted egress by default; users can switch a single knob to a default-deny allowlist mode.

## Decision Outcome

Chosen option: **default-open with an opt-in allowlist**, exposed as one declarative knob (`sandbox.egress.mode = "open" | "allowlist"`), defaulting to `open`. In `allowlist` mode a firewall enforces default-deny against named hosts; ADR-0044 places that firewall on the host side.

This is safe because the isolation boundary is the microVM, not the network (see [`ADR-0001-microvm-isolation-boundary.md`](./ADR-0001-microvm-isolation-boundary.md)). Open egress does **not** weaken VM isolation; its only cost is data-exfiltration exposure, which is a workload policy choice rather than a containment property. With open egress, the controls that matter are credential and mount scoping, not the network. See [`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md).

## Consequences

- Good: agent and developer workflows work out of the box with no network setup.
- Good: restriction remains one declarative switch away.
- Bad: the default permits data exfiltration; users must opt in to prevent it.
- Bad: two network modes to implement and test (open and enforced allowlist).

## Status

Accepted

Amended by [`ADR-0044-host-side-egress-and-reject-not-drop.md`](./ADR-0044-host-side-egress-and-reject-not-drop.md) — enforcement is **host-side**, not in-guest: a rule set inside a guest that may hold root could be torn down. The default-open decision and its single knob are unchanged. That ADR also requires a denied connection to fail fast rather than be silently dropped.

Amended by [`ADR-0064-egress-allowlist-enforcement-model.md`](./ADR-0064-egress-allowlist-enforcement-model.md) — the allowlist mechanism this ADR left abstract is now fixed (a per-VM host-side network namespace with a gating resolver), and the allowlist's string grammar gains wildcard and address/CIDR forms. The default-open decision and its single knob are unchanged.
