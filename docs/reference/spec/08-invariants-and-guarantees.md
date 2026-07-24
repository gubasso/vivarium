# 08 — Invariants and guarantees

The normative rules the product must uphold. Each is a guarantee callers and reviewers may rely on;
changing runtime, build, or composition behavior must preserve them. Each invariant links to the
page or decision that explains it. The keyword **must** marks a hard requirement.

## Isolation

- **N1 — Separate-kernel boundary.** A workspace **must** run behind a hardware-virtualization
  boundary with its own guest kernel; there is no shared-kernel mode. See
  [`../../decisions/ADR-0001-microvm-isolation-boundary.md`](../../decisions/ADR-0001-microvm-isolation-boundary.md).
- **N2 — Class, not tool.** The isolation boundary **must** be specified by capability class, not by
  a named virtual machine monitor. Any backend satisfying the class is admissible; none is part of
  the contract.

## Build and determinism

- **N3 — Pure build.** The sandbox build **must not** take any host-specific value as input. Given
  the same manifest closure and lockfile, it **must** evaluate to the same store output on any
  machine and at any later time. See
  [`04-composition-and-determinism.md`](04-composition-and-determinism.md).
- **N4 — Output hash is the freshness key.** Freshness **must** be derived from the build's store
  output, not a separately computed content digest. Changing any layer changes the inputs and thus
  the output.
- **N5 — Launch-time workspace path.** The working-directory host path **must** be injected when the
  VM launches, never built in, so no host path appears in any configuration file or build output. See
  [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md).

## Composition

- **N6 — No custom merge engine.** Layer composition **must** be performed by the NixOS module
  system, not a bespoke merge implementation. See
  [`../../decisions/ADR-0002-module-system-as-composition-engine.md`](../../decisions/ADR-0002-module-system-as-composition-engine.md).
- **N7 — One manifest per project.** A project **must** resolve to exactly one manifest, selected by
  the fixed precedence, and resolution **must** fail closed when none applies. See
  [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md).

## Network

- **N8 — Egress defaults to open.** Egress **must** default to unrestricted, with a single
  declarative knob to switch to a default-deny allowlist. Open egress **must not** be described as
  weakening the isolation boundary. See
  [`05-networking-and-egress.md`](05-networking-and-egress.md).

## Project environment

- **N9 — Project files are never modified.** vivarium **must not** modify a project's own files,
  including its development-environment configuration. The project's environment **must** work
  identically in or out of the sandbox. See
  [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md).

## Secrets and sharing

- **N10 — No secrets in the store.** A secret **must never** enter the build or the Nix store.
  Secrets are runtime-injected or encrypted-at-rest only. See
  [`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md).
- **N11 — Shared config is personal-data-free.** Committable configuration **must not** contain
  personal paths or plaintext secrets; such data belongs to the gitignored personal class. See
  [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md).

## State

- **N12 — User-based state.** All state **must** live under per-user XDG directories, split so that
  config is authored, data is pinned input, state is runtime, and cache is regenerable. No privileged
  or shared mutable state. See
  [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md).

## Config

- **N13 — Config is read-only to the tool.** vivarium **must not** write, create, or scaffold
  anything under the config root as a side effect of running a command; it only reads config. Any
  value the tool persists is state, data, or cache — never config. The one sanctioned write to a
  user-owned surface is an explicit, user-directed action that names its target (`viv init --write`,
  which targets the **state** registry, not config), is off by default, and is reversible. See
  [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md) and
  [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md).
