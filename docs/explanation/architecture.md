# Architecture

The mental model for how vivarium fits together. This page teaches the shape; for exact rules see the spec under [`../reference/spec/`](../reference/spec/README.md), and for the reasoning behind each choice see the decision records under [`../decisions/`](../decisions/).

## The box and what runs inside it

The central idea is a separation of two layers:

- **The box** — the sandbox vivarium builds: a microVM with its own kernel, its mounts, its network, and its security policy. vivarium owns this layer entirely.
- **What you do inside the box** — your project's own development environment, defined by the project and run within the guest. vivarium does not own or touch this layer.

Keeping these apart is what lets a project's environment stay portable — identical on bare metal, in CI, and inside a sandbox — while vivarium independently provides isolation around it. The full rationale is in [`../decisions/ADR-0008-two-layer-separation.md`](../decisions/ADR-0008-two-layer-separation.md).

## Two Nix evaluations

Because both layers are described in Nix, there are two evaluations, at different times and on different machines:

- The **outer** evaluation runs on the host at build time. It takes a manifest, resolves it to an image and pieces, merges them with the module system, and produces the VM. This is what `viv
  up` builds.
- The **inner** evaluation runs inside the guest at shell time. When a shell enters the mounted workspace, the project's own environment evaluates and loads.

They share nothing: separate configuration, separate lockfiles, separate evaluation moments. A newcomer's most common confusion is treating them as one; they are deliberately orthogonal.

## From manifest to running VM

The outer path is a short pipeline:

1. **Resolve the binding.** The project selects one manifest by a fixed precedence (see [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md)).
2. **Compile the manifest.** The TOML manifest becomes a generated flake whose module `imports` are the named image and pieces (see [`../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../decisions/ADR-0004-toml-manifest-compiles-to-flake.md)).
3. **Merge and build.** The module system merges the layers and Nix builds the VM. Identical inputs yield an identical store output, which is the freshness key (see [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md)).
4. **Launch.** The tool boots the VM and, at that moment, mounts the working directory — the one host-specific value, kept out of the pure build (see [`../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../decisions/ADR-0009-launch-time-workspace-path-injection.md)).

## Where the tool stops

vivarium does the orchestration — resolving, compiling, building, mounting, launching — and delegates the hard mechanisms to established building blocks: the virtualization backend provides the kernel and boundary, and the module system provides the merge. This is deliberate: the product's value is a clean, composable surface and sensible defaults over those mechanisms, not a reimplementation of them. The capability classes that keep the backend swappable are fixed by [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md).

## The boundary and its edges

The isolation model is a **second wall**. Inside a shared-kernel sandbox, one kernel or runtime bug is a host compromise; behind the microVM boundary an attacker must chain a guest-kernel escape _and_ a break of the virtual-machine monitor or the hardware boundary — a categorically harder exploit chain. That risk-class jump, not any single tool, is the reason the boundary is a microVM ([`../decisions/ADR-0001-microvm-isolation-boundary.md`](../decisions/ADR-0001-microvm-isolation-boundary.md), [`../decisions/ADR-0024-backend-security-requirements.md`](../decisions/ADR-0024-backend-security-requirements.md)).

A second wall is not zero risk, and knowing its edges is part of the mental model:

- **The host-side helpers are trusted surface.** The VMM process and the shared-filesystem daemon that serves the workspace both run on the host, so both are confined by construction — seccomp plus capability drop (N20) — rather than trusted. The concrete profile, and the guest-root-to-host-root escape class it closes (an unconfined virtiofsd, CVE-2026-47243), are fixed by [`../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md).
- **Network reach is not escape.** Open egress lets a compromised agent exfiltrate what it can already read; it does not weaken the boundary. That is why egress is a policy knob, not an isolation setting (N8, [`../decisions/ADR-0007-default-open-egress.md`](../decisions/ADR-0007-default-open-egress.md)).
- **The boundary protects only what stays inside it.** Files the guest writes into the workspace are later read on the host — by editors, hooks, CI, task runners. A hostile workspace file that a host tool executes walks _around_ the wall, not through it. vivarium never executes workspace content on the host itself (N9); users should extend the same caution to their own host tooling.
- **Out of scope.** CPU side channels and a hostile host are outside the threat model: the host is trusted, the guest is not.
