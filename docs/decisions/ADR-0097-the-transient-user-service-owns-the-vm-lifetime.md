# ADR-0097: The transient user service owns the VM lifetime

## Context and Problem Statement

The invoking command must detach while the VM, its share daemons, and its console reader remain one failure and cleanup domain. A transient scope is caller-parented, so it cannot be the manager-owned lifetime promised by detached start.

## Considered Options

- Keep a caller-owned transient scope.
- Run one manager-owned transient user service per `(project, target)`.
- Add a persistent vivarium daemon.

## Decision Outcome

Chosen option: one manager-owned transient user service per `(project, target)` — it gives the existing resource scope a detached owner without adding a daemon.

The service's main process is the Rust supervisor. It owns every `tokio::process::Child` handle and the console task. An unexpected child exit, fatal reader failure, explicit stop, process signal, or unit stop enters one cancellation path. Cancellation terminates and reaps all children before cleanup removes owned runtime artifacts and the supervisor exits.

The service is nested under `vivarium.slice`. It carries accounting, the accepted CPU weight, and the launch contract's `LimitNOFILE`. It carries no memory or I/O limit. `KillMode=control-group` makes unit stop a whole-group operation.

This decision narrowly amends ADR-0036's literal “transient scope” wording to “per-VM transient user unit/resource scope.” Its accounting, CPU-weight, no-memory-limit, no-I/O-limit, and whole-group teardown policy are unchanged. ADR-0056's user-session lifetime remains unchanged.

## Consequences

- Good: the systemd user manager, rather than the initiating CLI process, parents the supervised lifetime.
- Good: every exit route converges before cleanup.
- Bad: launch requires a usable systemd user manager and session bus.

## Status

Accepted
