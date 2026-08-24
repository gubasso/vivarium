# ADR-0020: Mount and host-config mirroring schema

## Context and Problem Statement

ADR-0017 allowed extra runtime mounts but deferred the declaration schema. Users need to mirror host configuration files and directories — dotfiles, read-only credentials, additional repositories — into the guest without leaking host paths into builds (N5) or secrets into the store (N10).

## Considered Options

- Imperative per-invocation CLI flags on `start`
- A per-project mount file inside the repository
- Declarative `[[mounts]]` entries in manifest and pieces, resolved at launch

## Decision Outcome

Chosen option: declarative `[[mounts]]` in manifest and pieces, resolved at launch — one schema for extra workspaces, config mirroring, and credential mounts, composing like every other list.

```toml
[[mounts]]
source   = "${HOME}/.config/foo"   # host path; ${VAR} expands host-side at launch
target   = "~/.config/foo"         # guest path; ~ expands to the guest home
readonly = true                    # default false

[env]
FOO_CONFIG = "~/.config/foo"       # runtime env injection (see 07)
```

- Per-side expansion: `source` resolves against the host environment, `target` against the guest home. Identity mounts therefore need no path translation even though the guest user stays `vivarium`. Caveat: mirroring is path-level only — file contents embedding host-absolute paths are not rewritten.
- Fail-fast validation at launch: an unset variable or missing host path aborts before boot with a legible error. Expanded paths are never build inputs and never recorded in shared artifacts (N5/N11).
- Extra `/workspaces/<name>` mounts use this same schema, closing ADR-0017's deferred item. Read-only credential mounts and runtime environment remain the sanctioned secret channels (ADR-0010); `[env]` values obey the deny-by-default rule (N17).
- Declarations from pieces and the manifest concatenate; duplicate `target` paths fail evaluation.

## Consequences

- Good: host-config mirroring with zero translation; one composable surface for all extra mounts.
- Bad: two expansion contexts (host vs guest) must be understood; content-level absolute paths can still break mirrored tools.

## Status

Accepted

Amended by [`ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](./ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — a `source` carries filesystem data only: a regular file or a directory, never a socket, FIFO, or device node, and never a host session directory (N24). Sockets have their own typed channel, because a share conveys an inode rather than a listener and this schema's `readonly` flags constrain file use rather than connection.

Amended by [`ADR-0021-typed-launch-channel-options-in-pieces.md`](./ADR-0021-typed-launch-channel-options-in-pieces.md) — the schema stands; the piece-side declaration surface is now typed `vivarium.mounts`/`vivarium.env` NixOS options (the TOML above remains the manifest surface, compiled into the same options), and shared layers are restricted to portable variables in mount sources.

Amended by [`ADR-0108-a-workspace-is-owned-by-one-manifest.md`](./ADR-0108-a-workspace-is-owned-by-one-manifest.md) — 2026-08-20, narrowing what this schema covers rather than changing it. A project tree moves to its own `[[workspaces]]` table, because it mirrors its host path and so has no `target` to declare, and at most one manifest may claim a given directory that way. Everything decided here governs the `target`-mounted kind unchanged, including the extra `/workspaces/<name>` repositories named above, and any number of manifests may mount one directory.

Amended by [`ADR-0110-the-workspace-is-an-ordinary-mount.md`](./ADR-0110-the-workspace-is-an-ordinary-mount.md) — a workspace compiles into this schema rather than beside it. The schema gains a row whose `target` equals its `source`, and that row's expanded path is a build input rather than launch data, which is the one place the "expanded paths are never build inputs" clause above no longer reaches.
