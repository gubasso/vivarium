# ADR-0048: Guest module only — vivarium owns the runner

## Context and Problem Statement

vivarium composes with microvm.nix, which ships a guest NixOS module, a host module, and a ready-made runner package (`config.microvm.declaredRunner`). Taking the generated runner would save writing a launch-argument generator. Nothing in the repository says whether vivarium takes it — and three accepted decisions quietly assume it does not.

## Considered Options

- **Adopt the host module and the generated runner** — system service units, a system-level state directory, upstream's own share-daemon supervisor.
- **Guest module only** — take guest-side system configuration; vivarium generates the launch and supervises every process itself.
- **Drop microvm.nix entirely** — build the guest from ordinary NixOS modules.

## Decision Outcome

Chosen option: **guest module only** — the generated runner cannot satisfy N5, and the host module cannot carry vivarium's confinement or state model.

- The generated runner writes each share's **host source path into the build output**. N5 and N19 forbid a host path in any build output, so `declaredRunner` cannot enact [`ADR-0009-launch-time-workspace-path-injection.md`](./ADR-0009-launch-time-workspace-path-injection.md).
- Upstream's share daemons run unsandboxed under a supervisor of its own. [`ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](./ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) makes vivarium's launch wrapper the N20 enforcement layer, and [`ADR-0036-host-resource-scoping-and-admission-control.md`](./ADR-0036-host-resource-scoping-and-admission-control.md) requires every process in one scope per (project, target). Neither survives a foreign supervisor.
- The host module assumes a system-level state directory and system service units; vivarium keeps per-project state under the XDG state root and runs per-user.
- vivarium therefore imports the **guest module** for guest-side system configuration — mount units for shares, volumes and the store, kernel and initrd wiring, network interfaces — and treats those values as **inputs to its own launch-argument generator**. Dropping the guest module too would mean reimplementing that plumbing for no gain in any invariant.

## Consequences

- Good: N5, N20, and the scope model become enforceable by construction rather than aspirational.
- Bad: guest mount units and host launch arguments share an upstream vocabulary with no stability policy. A rename yields a guest that boots with an empty workspace and no evaluation error — only a boot-and-mount acceptance test catches it.

## Status

Accepted

Presupposed by ADR-0027 and ADR-0036, both unbuildable if the upstream runner owns the launch; states how ADR-0009 is achieved. The concrete launch profile lives in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md); the lifecycle step that execs it is in [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md).
