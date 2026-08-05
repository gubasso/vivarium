# Launch and supervision

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

Nix builds a runner closure containing the selected virtual machine monitor, guest configuration, and helper binaries. The CLI hands runtime-only paths and declarations to that closure. vivarium owns the launch arguments and constructs a confinement profile for the monitor and every guest-facing helper rather than relying on ambient host configuration.

Confinement combines unprivileged execution, capability removal, seccomp, Landlock path rules, read-only sources where applicable, and one sharing daemon per share. Narrow `connect(2)` permissions admit the console socket and declared host credential sockets without mounting their parent runtime directory. Helper descriptor limits and service limits are declared together so diagnostic thresholds derive from configuration instead of host inheritance.

All children belong to one supervised VM lifetime. Build, monitor, sharing daemons, network helpers, console capture, readiness, cancellation, and cleanup have explicit ownership. Detached start requires a console reader attached before the first guest write and retained until VM exit, because the serial socket does not provide replay.

The normative boundaries are in the [workspace](../reference/spec/06-workspace-and-project-environment.md), [invariants](../reference/spec/08-invariants-and-guarantees.md), [lifecycle](../reference/spec/10-vm-lifecycle.md), [exec and shell](../reference/spec/12-exec-and-shell.md), [doctor](../reference/spec/13-doctor-and-health-checks.md), and [logging](../reference/spec/16-logging-and-diagnostics.md) specifications. Exact pinned backend facts live in [backend capabilities](../reference/backend-capabilities.md).

## Governing decisions

- [ADR-0001](../decisions/ADR-0001-microvm-isolation-boundary.md) — fixes the hardware-virtualization boundary this path must produce.
- [ADR-0009](../decisions/ADR-0009-launch-time-workspace-path-injection.md) — injects the workspace path at launch so it never becomes a build input.
- [ADR-0024](../decisions/ADR-0024-backend-security-requirements.md) — fixes the security requirements a backend has to satisfy.
- [ADR-0025](../decisions/ADR-0025-default-hypervisor-cloud-hypervisor.md) — selects the default virtual machine monitor.
- [ADR-0026](../decisions/ADR-0026-global-flags-and-config-precedence.md) — fixes the global flags the launch path reads.
- [ADR-0027](../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — fixes the confinement profile applied to the monitor and every helper.
- [ADR-0036](../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) — puts admission at launch instead of a background arbitrator.
- [ADR-0048](../decisions/ADR-0048-guest-module-only-vivarium-owns-the-runner.md) — keeps the runner vivarium-owned, exposing only a guest module.
- [ADR-0049](../decisions/ADR-0049-backend-is-a-closure-member.md) — makes the monitor and helpers closure members rather than host packages.
- [ADR-0056](../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md) — bounds the supervised lifetime by the user session.
- [ADR-0070](../decisions/ADR-0070-guest-console-capture-and-rotation.md) — fixes console capture, its destination, and its rotation boundary.
- [ADR-0078](../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — makes a backend fix a released pin move, which is why the pin is a launch-path concern.
- [ADR-0079](../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) — records the staffing posture behind those windows.
- [ADR-0081](../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md) — routes console output around the guest log daemon.
- [ADR-0093](../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md) — declares each helper's descriptor budget rather than inheriting it.
- [ADR-0095](../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md) — keeps measurement services out of the shipped image the launch path builds.

## Unresolved

- [Q-004](../plan/open-questions.md#q-004--which-supervision-model-keeps-every-child-and-the-console-reader-inside-the-vm-lifetime) and [slice 002](../plan/slices/002-secure-launch-and-supervision/README.md) own the supervision model and its enactment.
