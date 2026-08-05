# ADR-0005: Store all state under user-based XDG directories

## Context and Problem Statement

vivarium holds several kinds of state: the user's source of truth (images, pieces, manifests), pinned external module libraries, per-project runtime identity and logs, and derived build artifacts. These have different durability and backup needs and must not be conflated. The tool must also run entirely per-user, with no system-wide installation or root-owned state.

## Considered Options

- One directory for everything — a single `~/.vivarium/` mixing config, runtime, and cache.
- Four XDG roots by durability — split across the standard config, data, state, and cache directories, each owning one class of content.

## Decision Outcome

Chosen option: four XDG roots by durability. Config owns the user's source of truth (images, pieces, manifests, and the global config file) and is read-only to the tool. Data owns pinned external module libraries. State owns per-project VM identity, the project→manifest registry, and logs. Cache owns derived, regenerable build and evaluation artifacts. See [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) for what each root contains.

The split follows a single rule: config is authored, cache is derived, state is runtime, data is pinned inputs. Anything the user edits is config; anything the tool can rebuild is cache.

## Consequences

- Good: users can back up config alone; cache is safe to delete and regenerate.
- Good: fully per-user, no privileged installation or shared mutable state.
- Good: clear placement rule for every new artifact the tool produces.
- Bad: four locations to reason about instead of one.
- Bad: relies on XDG environment conventions being set sanely on the host.

## Status

Accepted

The four-class split stands. The project→manifest registry now lives in state, not the config file, and config is read-only to the tool — see [`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md).

Amended by [`ADR-0055-runtime-directory-is-required.md`](./ADR-0055-runtime-directory-is-required.md) — the XDG reliance recorded above still holds, but is now a specified precondition rather than an assumption: `$XDG_RUNTIME_DIR` is validated on every run that needs it, and its absence is a diagnosed failure (`77`) instead of undefined behavior. The other four roots keep their literal defaults.
