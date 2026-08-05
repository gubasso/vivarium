# State and lifecycle

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

vivarium separates authored configuration, pinned data, durable operational state, regenerable cache, and session runtime across the XDG roots. The tool never treats authored configuration as mutable state. Project identity is derived from the directory name and anchored by a self-ignored marker, while the per-user registry maps that identity to a live path and a selected manifest.

A build retained for a project becomes a generation pinned through a Nix profile or garbage-collector root. Runtime files describe only the live VM and its control surfaces. `start` ensures a VM exists and is fresh according to its build output; `stop` ends the runtime without removing generations or volumes; `destroy` removes project-owned vivarium state and removes volumes only when explicitly allowed by the command contract.

The sandbox is disposable. Durable project work remains in the host workspace or explicitly declared volumes, not in incidental guest state. VM lifetime is bounded by the user's session unless the host keeps the user service manager alive. Stale registry bindings are surfaced rather than silently reaped, and concurrent state changes use atomic writes and a total lock order.

Exact paths, states, command effects, and identity cases live in [config and XDG layout](../reference/spec/02-config-and-xdg-layout.md), [VM lifecycle](../reference/spec/10-vm-lifecycle.md), [generations](../reference/spec/11-generations-and-build-history.md), and [project identity](../reference/spec/15-project-identity.md).

## Governing decisions

- [ADR-0005](../decisions/ADR-0005-xdg-user-config-layout.md) — splits config, data, state, and cache by durability.
- [ADR-0013](../decisions/ADR-0013-vm-lifecycle-and-up.md) — fixes the lifecycle and what `viv up` means over it.
- [ADR-0014](../decisions/ADR-0014-build-generations-and-gc-roots.md) — retains builds as generations through a profile and garbage-collector roots.
- [ADR-0018](../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) — fixes what `start`, `stop`, and `destroy` each remove.
- [ADR-0029](../decisions/ADR-0029-project-identity-and-marker.md) — derives identity from the directory name and anchors it with a marker.
- [ADR-0030](../decisions/ADR-0030-vm-status-and-state-model.md) — fixes the state model `viv status` reports.
- [ADR-0043](../decisions/ADR-0043-identity-marker-lifecycle.md) — gives the marker's lifecycle to the lifecycle verbs.
- [ADR-0052](../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md) — fixes the state-root layout and how much of it is a contract.
- [ADR-0053](../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md) — fixes atomic writes and one total lock order.
- [ADR-0054](../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md) — surfaces a stale binding rather than reaping it.
- [ADR-0055](../decisions/ADR-0055-runtime-directory-is-required.md) — requires a real runtime directory instead of synthesizing one.
- [ADR-0056](../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md) — bounds VM lifetime by the user session.
- [ADR-0067](../decisions/ADR-0067-volume-prune-and-first-boot-home.md) — keeps pruning to orphans so active project state survives.
- [ADR-0080](../decisions/ADR-0080-the-sandbox-is-disposable.md) — makes the sandbox disposable, which is why durable work stays outside it.

## Unresolved

- Real supervision and lifetime enactment belongs to [slice 002](../plan/slices/002-secure-launch-and-supervision/README.md).
