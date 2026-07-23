# 00 — Goals and non-goals

## What vivarium is

vivarium runs a project inside its own **microVM** — a separate guest kernel behind a
hardware-virtualization boundary — described declaratively in Nix. It provides the "boring parts" of
a secure sandbox: resolving a project's configuration, building the VM reproducibly, mounting the
working directory, and wiring the network. Users describe a sandbox by composing reusable **images**
and **config pieces** through a single **manifest**; the same manifest produces the same VM anywhere.

The command-line tool is a thin wrapper over a Nix build and a virtual machine runner. It does not
reimplement isolation, kernels, or configuration merging — it orchestrates building blocks that
already provide them.

## Goals

- **Strong isolation.** A separate guest kernel behind hardware virtualization, suitable for running
  untrusted code and autonomous agents. See
  [`../../decisions/ADR-0001-microvm-isolation-boundary.md`](../../decisions/ADR-0001-microvm-isolation-boundary.md).
- **Declarative.** The sandbox is described entirely in configuration, not assembled by imperative
  steps.
- **Deterministic.** A pinned lockfile makes a build reproducible across machines and over time. See
  [`04-composition-and-determinism.md`](04-composition-and-determinism.md).
- **Composable.** Small, reusable images and pieces combine through manifests. See
  [`03-artifact-model.md`](03-artifact-model.md).
- **Non-invasive.** A project's own development environment runs inside the sandbox untouched. See
  [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md).
- **User-based.** All state lives under standard per-user directories; no privileged installation.
  See [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md).
- **Unobtrusive by default, restrictable on demand.** Open network by default, with an opt-in
  allowlist. See [`05-networking-and-egress.md`](05-networking-and-egress.md).

## Non-goals

- **Not a shared-kernel container runtime.** The boundary is a VM, not namespaces; there is no
  shared-kernel mode.
- **Not a general orchestrator.** vivarium manages per-user, per-project sandboxes, not clusters,
  scheduling, or multi-tenant fleets.
- **Not a replacement for a project's dev environment.** It runs that environment; it does not define
  or absorb it.
- **Not a secrets manager.** It refuses to place secrets in the build and integrates injection
  channels, but it does not store or rotate credentials. See
  [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md).
- **Not a packaging or publishing tool** for the artifacts it builds.

## Audience

Developers and teams comfortable with a Nix-based workflow who want reproducible, strongly isolated
sandboxes — particularly for running AI coding agents against real projects, where a deterministic
sandbox yields deterministic runs.
