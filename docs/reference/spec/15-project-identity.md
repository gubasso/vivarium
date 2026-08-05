# 15 — Project identity

Every per-project artifact vivarium writes is scoped by a single project-identity key, `<project-id>`: the build generations and volumes under the state root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`11-generations-and-build-history.md`](./11-generations-and-build-history.md)) and the running-VM runtime files under `$XDG_RUNTIME_DIR` ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). This page is the single source of truth for how that key is derived, persisted, and resolved. The decision and rationale are in [`../../decisions/ADR-0029-project-identity-and-marker.md`](../../decisions/ADR-0029-project-identity-and-marker.md).

## The name is the identity

`<project-id>` is the sanitized name of the project directory, not a hash of its path. It is human-readable (it appears in log lines and runtime paths), short enough to keep a Unix-socket path within its length limit, and assigned automatically — there is no flag and no manual step.

Sanitization. Lowercase the directory basename, replace every character outside `[a-z0-9-]` with `-`, collapse runs of `-`, trim leading/trailing `-`, then truncate to 48 characters and trim again if the cut left a trailing `-`. If the result is empty, use `project`.

The cap exists because the id is read, not just resolved: it appears in log lines ([`16-logging-and-diagnostics.md`](./16-logging-and-diagnostics.md)), in `viv status` output, in error messages, and in both the state and runtime paths — and a filesystem allows a basename several times longer than any of those stay legible at. It also keeps the control socket's absolute path clear of the operating system's Unix-socket length limit with room to spare, including for a `<target>` component longer than today's `default`. Two directories differing only past the cut sanitize to the same name; that is the ordinary collision above and takes the same suffix, because a truncation collision is indistinguishable from two identically-named directories.

Collision suffix. When the sanitized name is already held by a different, still-existing project, append the smallest free integer suffix: `api`, then `api-2`, `api-3`, and so on. The first holder keeps the bare name.

## The marker

So that identity survives a directory move or rename, vivarium anchors it in a marker it owns inside the project tree. The marker is created by the commands that start a VM, never by a read-only one — see [Resolving the identity](#resolving-the-identity) — and removed by `viv destroy`:

```text
<project>/.vivarium/
  .gitignore   # contains "*" — the directory ignores itself, including this file
  id           # the assigned <project-id>, one line
```

The `.gitignore` of `*` makes the whole `.vivarium/` directory invisible to git with zero user action — nothing to add to the project's own `.gitignore`, nothing committed. The marker is vivarium-owned and inert to the inner dev environment: it is never read by the project's own tooling and never changes how the project builds in or out of the sandbox. This is the sole exception to N9 (vivarium modifies no user-authored files), specified in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) (N9, N21) and [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md).

Because the marker is gitignored, it never travels through `git`: a fresh clone or a new `git worktree` starts with no marker and is therefore assigned its own identity.

## Identity versus binding

The marker carries identity only — the `<project-id>`. It is not the manifest binding. The project→manifest binding still lives exclusively in the per-user state registry, written only on an explicit `viv init --write`, and resolution precedence is unchanged (`--manifest` → `VIVARIUM_MANIFEST` → registry → fail closed; N7, [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md)).

Identity is tracked separately. vivarium keeps an identity index under the state root — `identity.toml`, one file, in the registry's format ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)) — that maps each assigned `id` to the live canonical path it currently occupies:

```toml
[[identities]]
id = "api-2"
path = "/home/alice/work/api-fork"
```

One file rather than a directory per id, because minting is "the smallest free suffix" — a global invariant that needs a global lock however the file is sharded. Unlike the registry record this shape is not a supported interface: nothing prints it and nothing invites a user to write it ([`../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md`](../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md)). It records the mapping and nothing more — no minting timestamp, and in particular no cached liveness flag, because whether a path still exists is what decides move-versus-copy below and must be read live. This index is ordinary tool-managed runtime state — written automatically like generations and runtime files, not gated behind `--write`, and only by the commands that mint (see below) — and it exists independently of the binding, so a project run through a `--manifest`/`VIVARIUM_MANIFEST` override (no registry entry) still has a stable identity. The index is what makes the resolution below deterministic.

## Resolving the identity

Resolution always runs; persistence does not. Every invocation resolves `<project-id>` — a read-only `viv status` needs it to locate the project's state just as much as `viv start` does. But only the shared ensure-running routine persists one: `viv start`, and `exec`/`shell` when they cold-start ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)). Every other command resolves in memory and writes nothing, which is what makes the read-only guarantee in [`14-exit-codes.md`](./14-exit-codes.md) literally true. Steps marked persist below are simply skipped when the caller does not mint; the resolved value is returned either way.

`viv init --write` does not mint either, despite writing. The project registry is keyed by the project's absolute path, not by `<project-id>` ([`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md)), so recording a binding needs no identity at all.

For the project directory at canonical (symlink-resolved) path `P` with sanitized basename `N`, vivarium resolves `<project-id>` as follows.

If the marker `.vivarium/id` exists (its value is `M`):

- No index entry for `M` → use `M`; persist the record `M → P`.
- The index says `M` lives at `P` → use `M` (the ordinary in-place run); nothing to persist.
- The index says `M` lives at a path that still exists and is not `P` → this directory is a copy; resolve to the smallest free suffix of `N`, and persist it by rewriting the marker and recording it.
- The index says `M` lives at a path that no longer exists → this is a move or rename; use `M` and persist the re-pointing `M → P`.

If there is no marker:

- The index already has an entry for path `P` → use its id and persist by rewriting the marker (this recovers a marker the user deleted).
- Otherwise resolve to `N` — suffixed if `N` is held by a different, still-existing project — and persist by writing the marker and recording it.

Either source can be rebuilt from the other: a deleted marker is recovered from the index entry for `P`, and a lost index entry is re-adopted from the marker. Assigning a brand-new id takes a short global lock on the identity index before the per-project `flock` ([`12-exec-and-shell.md`](./12-exec-and-shell.md)), so two concurrent first-time `viv start`s cannot mint divergent suffixes for the same directory. That edge is one link in the single total lock order specified in [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md) — registry before identity, identity before the per-target `flock` — and "short" is normative: the lock is released before any Nix build or VM boot ([`../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md`](../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md)).

One consequence is worth stating plainly: a resolved-but-unpersisted id is deterministic, not stable over time. Two copies of a project both resolve to `api-2` until one of them starts; once that one mints, the other resolves to `api-3`. Only minting settles a suffix, so a read-only command run in an unstarted copy may report a different id later.

## Behavior by scenario

Every row assumes the id is being minted — that is, the command is `viv start` or a cold-starting `exec`/`shell`. A read-only command resolves to the same value but leaves no marker and no index entry behind.

| Scenario                                                    | Resulting `<project-id>`                       |
| ----------------------------------------------------------- | ---------------------------------------------- |
| First `start` in `~/work/api`                               | `api`                                          |
| Re-run in place                                             | `api` (unchanged)                              |
| Move keeping the name (`~/work/api` → `~/archive/api`)      | `api` — state reattaches                       |
| Leaf-rename (`~/work/api` → `~/work/api2`)                  | `api` — the marker keeps the identity          |
| Second, different project also named `api`                  | `api-2`                                        |
| Copy (`cp -r api api-fork`) while the original still exists | `api-2` — copy is disambiguated                |
| Fresh `git clone` / `git worktree`                          | own name (marker not carried)                  |
| Directory name sanitizes to a taken id                      | smallest free suffix                           |
| Marker deleted                                              | recovered from the identity index for the path |
| Identity-index entry lost                                   | re-adopted from the marker                     |

## The target component

Per-project state and runtime paths carry a second component after `<project-id>`: `<target>`, the named VM instance within a project. A project has exactly one target, named `default`, in this version — there is no flag that selects another and no manifest key that declares one, so `<target>` is `default` in every path today.

The component exists in the paths anyway, deliberately. It is the one thing that cannot be added later without relocating every project's state: generations, volumes, and runtime files would all have to move. Reserving the segment now costs one directory level and keeps a future second VM per project (a variant with different resources, say) a purely additive change.

Two rules follow, and they are what keep the affordance honest:

- State and runtime paths use `<target>` symmetrically. `projects/<project-id>/<target>/` under the state root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)) and `vivarium/<project-id>/<target>/` under the runtime root ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). A path that keyed one and not the other would silently cap the project at one running VM regardless of what the state layout allows.
- Multiple sessions are not multiple targets. Several `exec`/`shell` sessions attach to one target's VM; they never create one ([`12-exec-and-shell.md`](./12-exec-and-shell.md)).

## Teardown

`viv destroy` ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)) removes the project's state under `projects/<project-id>/`, clears its identity-index entry, and removes the `.vivarium/` marker, so the next `viv start` in that directory is a clean first run. The manifest binding is left untouched — `destroy` never removes it ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)).
