# 09 — Glossary

Defined terms used across the vivarium documentation. Each term is defined once here; other pages use
it without redefining it.

- **Sandbox** — the isolated microVM vivarium builds and runs for a project: a guest kernel behind a
  hardware-virtualization boundary, plus its mounts, network, and configuration.

- **Guest** — the operating system running inside the sandbox VM, distinct from the **host** it runs
  on.

- **Host** — the machine on which vivarium and the virtualization backend run.

- **microVM** — a lightweight virtual machine with a minimal device model and its own kernel, used as
  the isolation unit.

- **Isolation boundary** — the security boundary between guest and host. In vivarium it is hardware
  virtualization with a separate guest kernel, specified by capability class rather than a named tool
  (see [`08-invariants-and-guarantees.md`](08-invariants-and-guarantees.md)).

- **Backend** — the concrete virtual machine monitor that realizes the isolation boundary. Any
  backend satisfying the boundary class is admissible; the specific backend is an implementation
  choice.

- **Image** — a composable VM base, expressed as a NixOS module, capturing a toolchain and base
  system. See [`03-artifact-model.md`](03-artifact-model.md).

- **Piece** — a small, single-purpose configuration fragment, expressed as a NixOS module, layered
  onto an image. See [`03-artifact-model.md`](03-artifact-model.md).

- **Manifest** — the unifier that names one image plus an ordered set of pieces and policy knobs; the
  single source of truth a project binds to. Authored as TOML, compiled to a generated flake. See
  [`03-artifact-model.md`](03-artifact-model.md).

- **Leaf** — the highest-specificity layer in a composition: the manifest's own settings, which use
  normal priority and so override image defaults. See
  [`04-composition-and-determinism.md`](04-composition-and-determinism.md).

- **Binding** — the association between a project and its manifest, resolved by a fixed precedence.
  See [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md).

- **Workspace** — the user's working directory, mounted read-write into the guest at a fixed
  location. See [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md).

- **Inner environment** — the project's own development environment, owned by the repository and run
  inside the sandbox, independent of vivarium.

- **Outer / inner evaluation** — the two Nix evaluations: the outer builds the sandbox from the
  manifest at build time; the inner builds the project's development environment inside the guest at
  shell time.

- **Egress** — outbound network traffic from the guest, governed by the `sandbox.egress.mode` knob.
  See [`05-networking-and-egress.md`](05-networking-and-egress.md).

- **Closure** — the complete set of store paths a build depends on; what ships with a built VM.

- **Store** — the content-addressed Nix store holding build outputs. It is world-readable, which is
  why secrets must never enter it (see
  [`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md)).
