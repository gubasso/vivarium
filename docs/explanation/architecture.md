# Architecture

The mental model for how nixvault fits together. This page teaches the shape; for exact rules see the
spec under [`../reference/spec/`](../reference/spec/README.md), and for the reasoning behind each
choice see the decision records under [`../decisions/`](../decisions/).

## The box and what runs inside it

The central idea is a separation of two layers:

- **The box** — the sandbox nixvault builds: a microVM with its own kernel, its mounts, its network,
  and its security policy. nixvault owns this layer entirely.
- **What you do inside the box** — your project's own development environment, defined by the project
  and run within the guest. nixvault does not own or touch this layer.

Keeping these apart is what lets a project's environment stay portable — identical on bare metal, in
CI, and inside a sandbox — while nixvault independently provides isolation around it. The full
rationale is in
[`../decisions/ADR-0008-two-layer-separation.md`](../decisions/ADR-0008-two-layer-separation.md).

## Two Nix evaluations

Because both layers are described in Nix, there are two evaluations, at different times and on
different machines:

- The **outer** evaluation runs on the host at build time. It takes a manifest, resolves it to an
  image and pieces, merges them with the module system, and produces the VM. This is what `nixvault
  up` builds.
- The **inner** evaluation runs inside the guest at shell time. When a shell enters the mounted
  workspace, the project's own environment evaluates and loads.

They share nothing: separate configuration, separate lockfiles, separate evaluation moments. A newcomer's
most common confusion is treating them as one; they are deliberately orthogonal.

## From manifest to running VM

The outer path is a short pipeline:

1. **Resolve the binding.** The project selects one manifest by a fixed precedence
   (see [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md)).
2. **Compile the manifest.** The TOML manifest becomes a generated flake whose module `imports` are
   the named image and pieces
   (see [`../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../decisions/ADR-0004-toml-manifest-compiles-to-flake.md)).
3. **Merge and build.** The module system merges the layers and Nix builds the VM. Identical inputs
   yield an identical store output, which is the freshness key
   (see [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md)).
4. **Launch.** The tool boots the VM and, at that moment, mounts the working directory — the one
   host-specific value, kept out of the pure build
   (see [`../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../decisions/ADR-0009-launch-time-workspace-path-injection.md)).

## Where the tool stops

nixvault does the orchestration — resolving, compiling, building, mounting, launching — and delegates
the hard mechanisms to established building blocks: the virtualization backend provides the kernel
and boundary, and the module system provides the merge. This is deliberate: the product's value is a
clean, composable surface and sensible defaults over those mechanisms, not a reimplementation of
them. The capability classes that keep the backend swappable are fixed by
[`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md).
