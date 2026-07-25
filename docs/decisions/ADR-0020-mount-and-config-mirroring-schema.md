# ADR-0020: Mount and host-config mirroring schema

## Context and Problem Statement

ADR-0017 allowed extra runtime mounts but deferred the declaration schema. Users need to mirror
host configuration files and directories — dotfiles, read-only credentials, additional repositories —
into the guest without leaking host paths into builds (N5) or secrets into the store (N10).

## Considered Options

- Imperative per-invocation CLI flags on `start`
- A per-project mount file inside the repository
- Declarative `[[mounts]]` entries in manifest and pieces, resolved at launch

## Decision Outcome

Chosen option: **declarative `[[mounts]]` in manifest and pieces, resolved at launch** — one schema
for extra workspaces, config mirroring, and credential mounts, composing like every other list.

```toml
[[mounts]]
source   = "${HOME}/.config/foo"   # host path; ${VAR} expands host-side at launch
target   = "~/.config/foo"         # guest path; ~ expands to the guest home
readonly = true                    # default false

[env]
FOO_CONFIG = "~/.config/foo"       # runtime env injection (see 07)
```

- **Per-side expansion:** `source` resolves against the host environment, `target` against the
  guest home. Identity mounts therefore need no path translation even though the guest user stays
  `vivarium`. Caveat: mirroring is path-level only — file contents embedding host-absolute paths
  are not rewritten.
- **Fail-fast validation at launch:** an unset variable or missing host path aborts before boot
  with a legible error. Expanded paths are never build inputs and never recorded in shared
  artifacts (N5/N11).
- Extra `/workspaces/<name>` mounts use this same schema, closing ADR-0017's deferred item.
  Read-only credential mounts and runtime environment remain the sanctioned secret channels
  (ADR-0010); `[env]` values obey the deny-by-default rule (N17).
- Declarations from pieces and the manifest concatenate; duplicate `target` paths fail evaluation.

## Consequences

- Good: host-config mirroring with zero translation; one composable surface for all extra mounts.
- Bad: two expansion contexts (host vs guest) must be understood; content-level absolute paths can
  still break mirrored tools.

## Status

Accepted
