# Architecture

This overview describes vivarium's accepted architecture, not its implementation state; consult [implementation status](../reference/implementation-status.md) before relying on a command or runtime component.

vivarium separates two environments. The outer environment is the reproducible microVM boundary that vivarium builds and launches. The inner environment is the project's own development setup, evaluated inside the guest and left usable outside it. Project-authored files remain on the host and are shared into the guest; vivarium-owned state and build artifacts live under per-user XDG roots.

The command-line tool resolves one manifest, composes its image and pieces through the NixOS module system, materializes a generated flake, and asks Nix to build a runner closure. At launch, host-specific paths and other runtime declarations are injected without becoming build inputs. The runner starts the closure-owned virtual machine monitor and guest-facing helpers inside one supervised lifetime.

vivarium owns manifest resolution, state, orchestration, confinement construction, and user-facing contracts. Nix owns evaluation, module merging, builds, locks, and store closure semantics. The selected virtual machine monitor and sharing daemons own virtualization and device transport. This boundary keeps vivarium a thin orchestrator rather than a replacement for any upstream mechanism.

## Subsystems

- [Configuration and composition](./configuration-and-composition.md) covers selection, artifacts, module merging, generated flakes, locks, and validation.
- [State and lifecycle](./state-and-lifecycle.md) covers XDG state, sandbox keys, generations, runtime state, and teardown.
- [Launch and supervision](./launch-and-supervision.md) covers the build-to-launch handoff, confinement, helpers, and console ownership.
- [Shared filesystems](./shared-filesystems.md) covers workspace, configuration, and host-store shares.
- [Guest store and volumes](./guest-store-and-volumes.md) covers persistent volumes and the overlay guest store.
- [Resources and capacity](./resources-and-capacity.md) covers elastic ceilings, admission, reporting, and reclaim.
- [Networking and egress](./networking-and-egress.md) covers per-VM networking and name-level policy enforcement.
- [Guest control and secrets](./guest-control-and-secrets.md) covers control sessions, credential relay, and secret boundaries.
- [CLI and diagnostics](./cli-and-diagnostics.md) covers orchestration, output, errors, logs, inspection, and stability.

The [normative invariants](../reference/spec/08-invariants-and-guarantees.md) govern every subsystem. ADRs record why choices were made; the linked subsystem pages describe how those choices fit together now.
