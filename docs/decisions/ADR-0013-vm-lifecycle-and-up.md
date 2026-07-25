# ADR-0013: VM lifecycle and `viv up` semantics

## Context and Problem Statement

`viv up` builds and boots a project's sandbox, and `exec`/`shell` "start it if needed." Left
unspecified, that invites surprising behavior: does re-running `up` rebuild and kill a running VM,
block the terminal, or silently no-op? We need one lifecycle model that is safe to run twice and does
not destroy in-guest work.

## Considered Options

- **Foreground/compose model** — `up` streams the console and blocks; a config change auto-recreates
  the VM (like `docker compose up`).
- **Background/vagrant model** — `up` boots a background VM and returns; it is idempotent and never
  replaces a running VM without an explicit request.
- **Rebuild-always** — every `up` re-evaluates and restarts unconditionally.

## Decision Outcome

Chosen option: **background/vagrant model** — the VM is a persistent resource `exec`/`shell` attach
to, and `up` is idempotent.

- **Detached by default.** `up` boots and returns; `--attach` streams the console, where `Ctrl-C`
  detaches without stopping the VM. There is no `--detach` flag (it would be the default).
- **Idempotent.** A fresh, already-running VM re-ups to a no-op exiting `0` (N15). The "ensure
  running" step is the shared routine `exec`/`shell` reuse.
- **Non-destructive staleness.** When a layer changed, `up` builds the fresh output but does **not**
  replace a running VM; it warns and names the fix. `--rebuild` stops and replaces the VM (volumes
  preserved); `--no-rebuild` boots the last build without evaluating.

## Consequences

- Good: safe to run repeatedly; long-lived guest processes are never dropped implicitly.
- Good: one "ensure running" primitive serves `up`, `exec`, and `shell`.
- Bad: applying a changed manifest to a running VM needs an explicit `--rebuild` (or `down`+`up`).

## Status

Accepted

Amended by
[`ADR-0018-lifecycle-verbs-and-teardown-boundary.md`](ADR-0018-lifecycle-verbs-and-teardown-boundary.md) —
the verbs were renamed `up` → `start` and `down` → `stop`, and teardown gained a separate
`destroy`; every behavior decided here (detached-default, idempotency, non-destructive staleness,
`--rebuild`/`--no-rebuild`) stands unchanged under the new names.

Specified in [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md).
Partially discharges the lifecycle-model work; the VM-identity key is decided separately.
