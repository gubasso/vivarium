# ADR-0053: State-file atomicity and lock ordering

## Context and Problem Statement

[`ADR-0052-state-root-file-layout-and-schema-visibility.md`](./ADR-0052-state-root-file-layout-and-schema-visibility.md) gives the two state files concrete shapes, which makes concurrent access a real question rather than an abstract one: several `viv` processes can run at once, and both files are read-modify-write. Two locks already exist elsewhere — the per-target `flock` in [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md) and the per-project Nix profile — so a partial order was already implied and never written down, which is how deadlocks arrive.

## Considered Options

- No locking; rely on `rename` atomicity and let the last writer win.
- A lock per file, with no stated order between locks.
- Sidecar lock files plus one documented total order across every lock vivarium takes.

## Decision Outcome

Chosen option: **sidecar locks under a total order** — losing a binding to a lost update is silent, and an undocumented order between four locks is a deadlock waiting for the second writer.

- **Write**: serialize to a temp file in the same directory, `sync_all`, `rename` over the target, then fsync the parent directory. A reader therefore sees the old file or the new one, never a partial one.
- **Permissions**: both files `0600`, state root `0700`. Absolute paths are weak but real information about a user's filesystem.
- **Lock**: a sidecar `<file>.lock` with `flock(2)` through `std::fs::File::lock` — no new dependency. Shared for read-only diagnostics, exclusive for writers; a lock that cannot be taken promptly is `75`, not a hang.
- **Order**: registry → identity → per-target `flock` → Nix profile. Never acquire upward, release in reverse, and hold no state lock across a VM boot or a Nix build.

## Consequences

- Good: no torn file, no lost update, and no cycle to reason about at each new lock site.
- Good: `flock` releases on process death, so a crash cannot leave the registry wedged.
- Bad: an MSRV of 1.89 or later, if one is ever declared.
- Bad: every future lock must be placed in this order explicitly; the rule is only as good as its upkeep.

## Status

Accepted

Specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), with the exit codes in [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
