# ADR-0030: `viv status` and the VM lifecycle state model

## Context and Problem Statement

vivarium had no verb to report a project VM's operational state, and its lifecycle vocabulary
([`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md)) left `failed`/
`crashed` and a `starting` transitional undecided, and listed `stale` as a peer state. A `status`
command needs a settled, closed enumeration to report against.

## Considered Options

- Report a backend-native vocabulary (libvirt/virsh, systemd) straight through.
- A minimal set (`absent`/`built`/`running`) with no failure or transitional states.
- A small vivarium-owned set: resting states plus `starting`/`stopping` transitional and one
  `failed` state, with staleness modelled as a flag.

## Decision Outcome

Chosen option: **a small vivarium-owned state set**, surfaced by `viv status`.

- **States:** `absent`, `built` (also "stopped"), `starting`, `running`, `stopping`, `failed`.
  `starting` and `stopping` are transitional; the rest rest.
- **Staleness is a boolean on `running`**, not a state — freshness derives from the store output
  path (N4), orthogonal to liveness, so a stale VM is still `running`, only drifted.
- **`failed`, not `crashed`:** `failed` covers boot failure, abnormal VMM/guest exit, and broken
  runtime records; the narrow `crashed` becomes a *reason* field. A clean `stop`/`destroy` tears
  down runtime markers, so "markers present + process dead" distinguishes `failed` from `built`.
- **`viv status`** is read-only, project-local by default; `-g`/`--global` (scoped to `status`, on
  YAGNI grounds) enumerates the state registry. Any reported state — including `failed` — exits `0`;
  the state is data, not a command failure.

## Consequences

- Good: a closed, backend-independent vocabulary; `status` output is stable and scriptable.
- Good: staleness-as-flag avoids state explosion and matches N4.
- Bad: telling `failed` from `built` depends on the marker-teardown invariant holding in `stop`.

## Status

Accepted

Applied in [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md),
[`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), and
[`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
