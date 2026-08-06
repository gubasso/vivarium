<!-- markdownlint-configure-file {"MD043": {"headings": ["?", "## Goal", "## Appetite", "## Core", "## In scope", "## Out of scope", "## Governed by", "## Acceptance", "## Rabbit holes", "## Done when", "## Revisions"], "match_case": true}} -->

# 002 — Secure launch and supervision

## Goal

Make the first real `viv start` launch and supervise the VM and helpers under the specified confinement and lifetime boundaries.

## Appetite

4 implementation sessions.

## Core

N20 is true by construction for the VMM and every guest-facing helper, all children are supervised and cancelled together, and detached start retains a console reader from before the first guest write until VM exit. These guarantees share one launch-construction and supervisor-ownership seam; the core is bounded to three sessions, leaving the derived doctor threshold and broader negative-test matrix as ordered remainder, so the slice remains one funded unit.

## In scope

- Construct seccomp, Landlock, and capability policy with focused negative tests.
- Add exact console-socket and declared agent-socket `connect(2)` legs.
- Pass explicit virtiofsd `--rlimit-nofile` and pin the unit's `LimitNOFILE`.
- Implement `tokio::process` supervision, stream capture, cancellation, readiness, and cleanup.
- Own the console reader for the complete VM lifetime.
- Derive the doctor threshold from the declared descriptor budget.

## Out of scope

- Guest-agent protocol implementation.
- Networking.
- Alternate VMMs.
- New product invariants.

## Governed by

- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — defines N20 and the cross-boundary guarantees.
- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — defines share confinement.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — defines runtime lifetime and cleanup.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — defines control-socket readiness.
- [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) — defines descriptor diagnostics.
- [`../../../reference/spec/16-logging-and-diagnostics.md`](../../../reference/spec/16-logging-and-diagnostics.md) — defines console capture.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — defines the failure categories this slice's two binaries return.
- [`../../../explanation/launch-and-supervision.md`](../../../explanation/launch-and-supervision.md) — owns the launch topology.
- [`../../../explanation/shared-filesystems.md`](../../../explanation/shared-filesystems.md) — owns share-daemon budgets.
- [`../../../reference/backend-capabilities.md`](../../../reference/backend-capabilities.md) — owns pinned argument shapes.
- [`../../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — fixes the confinement profile.
- [`../../../decisions/ADR-0033-error-handling-and-exit-codes.md`](../../../decisions/ADR-0033-error-handling-and-exit-codes.md) — fixes the error-type stack and how a typed error becomes a process code.
- [`../../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md`](../../../decisions/ADR-0036-host-resource-scoping-and-admission-control.md) — fixes per-VM scope ownership.
- [`../../../decisions/ADR-0048-guest-module-only-vivarium-owns-the-runner.md`](../../../decisions/ADR-0048-guest-module-only-vivarium-owns-the-runner.md) — assigns launch construction to vivarium.
- [`../../../decisions/ADR-0070-guest-console-capture-and-rotation.md`](../../../decisions/ADR-0070-guest-console-capture-and-rotation.md) — fixes console ownership.
- [`../../../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md`](../../../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md) — fixes the guest producer path.
- [`../../../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md`](../../../decisions/ADR-0093-the-share-descriptor-budget-is-declared.md) — requires an explicit descriptor budget.

## Acceptance

When launch arguments are constructed, the runner SHALL admit only the sanctioned processes, resources, syscalls, and paths.

Where an agent channel is declared, the confinement profile SHALL permit the exact host-agent connection and SHALL NOT permit the runtime directory as a mount.

If any child fails or cancellation occurs, then the supervisor SHALL terminate the group and leave the runtime directory clean.

When `viv start` detaches, the console reader attached before the first guest write SHALL survive until VM exit.

## Rabbit holes

- Blanket runtime-directory access — escape: construct exact socket-path rules.
- A detached reader proves transport but not lifetime — escape: add a lifetime assertion at the supervision seam.
- Supervision design expands — escape: limit Q-004's ADR to child ownership and cancellation.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, Q-004 exits through an ADR opened inside this slice, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
