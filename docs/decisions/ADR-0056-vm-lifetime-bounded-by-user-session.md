# ADR-0056: A VM's lifetime is bounded by the user session

## Context and Problem Statement

[`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md) promises a VM is a background resource that outlives the command that started it, and says nothing about whether it outlives the session. It does not: [`ADR-0036-host-resource-scoping-and-admission-control.md`](./ADR-0036-host-resource-scoping-and-admission-control.md) places every VM in a transient systemd user scope, whose manager exits at the user's final logout, taking the runtime root ([`ADR-0055-runtime-directory-is-required.md`](./ADR-0055-runtime-directory-is-required.md)) with it. A reader of "persistent" would guess otherwise.

## Considered Options

- Leave the boundary unspecified.
- Document it in a guide only.
- State it in the lifecycle contract and report the host's setting as a soft check.
- Require lingering as a hard preflight prerequisite.

## Decision Outcome

Chosen option: state it and report the setting. The lifecycle page gains one sentence — a VM does not survive the user's final logout unless the host keeps the user's session manager alive — and `viv doctor` gains a soft host check, `host-linger`, so a user learns the setting before losing a session's work rather than after. The check warns only while a VM is actually running: lingering is off by default on most hosts, so an unconditional warning would be noise rather than a signal.

Requiring lingering was rejected: single-session use is the primary case, and refusing to launch there would trade a documented boundary for a broken default. Leaving it unspecified was rejected because "persistent" reads as indefinite.

vivarium never enables lingering itself: changing a host-wide per-user setting as a side effect of starting one VM is outside the boundary ADR-0036 drew.

## Consequences

- Good: the VM and its runtime root die together, so a logout leaves no markers behind and the next login reports built, not failed — the discriminator in [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md) stays true.
- Good: no session-manager machinery enters vivarium; the lever is the host's.
- Bad: a user who wants VMs to span logouts must know to enable lingering, and the soft check is the only prompt.
- Bad: the guarantee is now host-conditional, so the lifecycle contract has one more precondition to state.

## Status

Accepted

The boundary sentence lives in [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md); the check is `host-linger` in [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
