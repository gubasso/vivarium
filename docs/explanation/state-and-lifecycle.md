# State and lifecycle

This page describes implemented behavior — `viv start`, `viv status`, and `viv stop` run, the selected manifest names the sandbox, every build is retained as a rooted generation, and `viv status -g` enumerates the fleet; see [implementation status](../reference/implementation-status.md) for the narrower gaps that remain.

vivarium separates authored configuration, pinned data, durable operational state, regenerable cache, and session runtime across the XDG roots. The tool never treats authored configuration as mutable state. The manifest name keys a sandbox, and workspace ownership is derived from explicit declarations in the personal manifest library.

A build retained for a project becomes a generation pinned through a Nix profile or garbage-collector root. Runtime files describe only the live VM and its control surfaces. `start` ensures a VM exists and is fresh according to its build output; `stop` ends the runtime without removing generations or volumes; `destroy` removes project-owned vivarium state and removes volumes only when explicitly allowed by the command contract.

`failed` is one state with many causes, and [`10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md) carries the cause as a reason rather than as a state. That vocabulary grows one cause at a time: vivarium names a reason only for a condition its discriminator can actually tell apart, reports the cause it can defend for the rest, and forwards the underlying read or parse error as diagnostic text rather than inventing a value a consumer would branch on. Today the discriminator reaches `failed` from four conditions — a dead recorded process, a live pid with no owning unit, an inactive unit behind live records, and an unreadable `vm.pid` — and names only the first, so the other three are unclassified by that rule rather than by oversight. Widening the set is additive, because the specification fixes the set as open and obliges a consumer to tolerate a value it does not know; narrowing what a consumer may already assume is not — which is why retracting the never-`null` guarantee on `resources` for the one unreadable-record case was recorded in the command surface with a compatibility note rather than slipped in, the exit `Q-015` chose when slice 025 consumed it.

The sandbox is disposable. Durable project work remains in the host workspace or explicitly declared volumes, not in incidental guest state. VM lifetime is bounded by the user's session unless the host keeps the user service manager alive. Retained state whose manifest is absent is surfaced rather than silently reaped, and mutable state uses atomic writes and the remaining per-sandbox locks.

Exact paths, ownership resolution, states, and command effects live in [config and XDG layout](../reference/spec/02-config-and-xdg-layout.md), [VM lifecycle](../reference/spec/10-vm-lifecycle.md), and [generations](../reference/spec/11-generations-and-build-history.md).

## Governing decisions

- [ADR-0005](../decisions/ADR-0005-xdg-user-config-layout.md) — splits config, data, state, and cache by durability.
- [ADR-0013](../decisions/ADR-0013-vm-lifecycle-and-up.md) — fixes the lifecycle and what `viv up` means over it.
- [ADR-0014](../decisions/ADR-0014-build-generations-and-gc-roots.md) — retains builds as generations through a profile and garbage-collector roots.
- [ADR-0018](../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md) — fixes what `start`, `stop`, and `destroy` each remove.
- [ADR-0107](../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) — keys each sandbox on its manifest and removes the old identity machinery.
- [ADR-0030](../decisions/ADR-0030-vm-status-and-state-model.md) — fixes the state model `viv status` reports.
- [ADR-0052](../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md) — fixes the state-root layout and how much of it is a contract.
- [ADR-0053](../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md) — fixes atomic writes and one total lock order.
- [ADR-0054](../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md) — supplies the surviving report-rather-than-reap judgment for orphaned state.
- [ADR-0055](../decisions/ADR-0055-runtime-directory-is-required.md) — requires a real runtime directory instead of synthesizing one.
- [ADR-0056](../decisions/ADR-0056-vm-lifetime-bounded-by-user-session.md) — bounds VM lifetime by the user session.
- [ADR-0067](../decisions/ADR-0067-volume-prune-and-first-boot-home.md) — keeps pruning to orphans so active project state survives.
- [ADR-0080](../decisions/ADR-0080-the-sandbox-is-disposable.md) — makes the sandbox disposable, which is why durable work stays outside it.

## Unresolved

- Admission control before launch belongs to [slice 026](../plan/slices/026-a-start-checks-the-room/README.md); `viv start` does not run it.
