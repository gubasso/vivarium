# ADR-0062: The override lock is per manifest, and `viv update` refuses under it

## Context and Problem Statement

[`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) gave a team its shared pin as a read-only `flake.lock` in the config root — but the config root holds every manifest a user has, so one file pins every project. That is the global lock the same ADR rejected one layer up. Nor does anything say what `viv update` does when the file it writes is not the file read.

## Considered Options

- One override lock at the config root, covering every project.
- One override lock per manifest, beside its `default.toml`.
- Let `viv update` write the shadowed data-root lock anyway.

## Decision Outcome

Chosen option: one override lock per manifest, at `manifests/<name>/flake.lock`, and `viv update` refuses while it is in force.

- Per manifest, for the reason ADR-0059 made the tool-owned lock per target: a pin that covers everything makes adopting one team's pin an unannounced pin of every unrelated project. No new concept is needed — ADR-0045 already makes a non-member file in a library directory readable by the tool and invisible to `viv manifest list`. It therefore requires the directory form, which ADR-0063 requires anyway.
- `viv update` refuses, writing nothing — not even the shadowed lock. Writing it manufactures a pin nothing reads today that becomes effective the moment the override is removed: an unannounced input jump, which is N3's failure mode.
- `78`, not a new code. The condition is decidable from the config root alone, the `78` side of the line spec/03 draws, and `78` already names a configuration in force that makes a command impossible. `77` is scoped to host permission failure and would make "fix the mode bits" indistinguishable from "your team pinned this".
- Not a silent success. Exit `0` already means "an update that moves nothing".
- The generation record retains the lock in force.

## Consequences

- Good: adopting a shared pin costs exactly the projects that use that manifest.
- Good: "exactly one effective lockfile" stays literally true — no shadow pin waits to activate.
- Bad: one pin across several manifests means one file per manifest.
- Bad: `update` is unavailable rather than inert; the remedy lies outside vivarium.

## Status

Accepted

Amends [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — the override lock moves from the config root to `manifests/<name>/flake.lock`, and `viv update` refuses (`78`) rather than writing the lock it shadows. The tool-owned per-target lock, and everything else the ADR decides, is unchanged.

Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md), and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
