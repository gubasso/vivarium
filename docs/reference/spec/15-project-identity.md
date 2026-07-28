# 15 — Project identity

Every per-project artifact vivarium writes is scoped by a single **project-identity key**, `<project-id>`: the build generations and volumes under the state root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`11-generations-and-build-history.md`](./11-generations-and-build-history.md)) and the running-VM runtime files under `$XDG_RUNTIME_DIR` ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). This page is the single source of truth for how that key is derived, persisted, and resolved. The decision and rationale are in [`../../decisions/ADR-0029-project-identity-and-marker.md`](../../decisions/ADR-0029-project-identity-and-marker.md).

## The name is the identity

`<project-id>` is the **sanitized name of the project directory**, not a hash of its path. It is human-readable (it appears in log lines and runtime paths), short enough to keep a Unix-socket path within its length limit, and assigned automatically — there is no flag and no manual step.

**Sanitization.** Lowercase the directory basename, replace every character outside `[a-z0-9-]` with `-`, collapse runs of `-`, and trim leading/trailing `-`. If the result is empty, use `project`.

**Collision suffix.** When the sanitized name is already held by a _different, still-existing_ project, append the smallest free integer suffix: `api`, then `api-2`, `api-3`, and so on. The first holder keeps the bare name.

## The marker

So that identity survives a directory move or rename, vivarium anchors it in a marker it owns inside the project tree:

```text
<project>/.vivarium/
  .gitignore   # contains "*" — the directory ignores itself, including this file
  id           # the assigned <project-id>, one line
```

The `.gitignore` of `*` makes the whole `.vivarium/` directory invisible to git with **zero user action** — nothing to add to the project's own `.gitignore`, nothing committed. The marker is vivarium-owned and **inert to the inner dev environment**: it is never read by the project's own tooling and never changes how the project builds in or out of the sandbox. This is the sole exception to N9 (vivarium modifies no _user-authored_ files), specified in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md) (N9, N21) and [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md).

Because the marker is gitignored, it never travels through `git`: a fresh clone or a new `git worktree` starts with no marker and is therefore assigned its own identity.

## Identity versus binding

The marker carries **identity only** — the `<project-id>`. It is **not** the manifest binding. The project→manifest binding still lives exclusively in the per-user state registry, written only on an explicit `viv init --write`, and resolution precedence is unchanged (`--manifest` → `VIVARIUM_MANIFEST` → registry → fail closed; N7, [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md)).

Identity is tracked separately. vivarium keeps an **identity index** under the state root that maps each assigned `id` to the **live canonical path** it currently occupies. This index is ordinary tool-managed runtime state — written automatically like generations and runtime files, not gated behind `--write` — and it exists independently of the binding, so a project run through a `--manifest`/`VIVARIUM_MANIFEST` override (no registry entry) still has a stable identity. The index is what makes the resolution below deterministic.

## Resolving the identity

On every invocation, for the project directory at canonical (symlink-resolved) path `P` with sanitized basename `N`, vivarium resolves `<project-id>` as follows.

**If the marker `.vivarium/id` exists** (its value is `M`):

- No index entry for `M` → re-adopt it (record `M → P`).
- The index says `M` lives at `P` → use `M` (the ordinary in-place run).
- The index says `M` lives at a path that **still exists** and is not `P` → this directory is a **copy**; mint the smallest free suffix of `N`, rewrite the marker, and record it.
- The index says `M` lives at a path that **no longer exists** → this is a **move or rename**; re-point `M → P` and use `M`.

**If there is no marker:**

- The index already has an entry for path `P` → re-adopt its id and rewrite the marker (this recovers a marker the user deleted).
- Otherwise assign `N` — suffixed if `N` is held by a different, still-existing project — write the marker, and record it.

Either source can be rebuilt from the other: a deleted marker is recovered from the index entry for `P`, and a lost index entry is re-adopted from the marker. Assigning a brand-new id takes a short global lock on the identity index before the per-project `flock` ([`12-exec-and-shell.md`](./12-exec-and-shell.md)), so two concurrent first-time `viv start`s cannot mint divergent suffixes for the same directory.

## Behavior by scenario

| Scenario                                                    | Resulting `<project-id>`                       |
| ----------------------------------------------------------- | ---------------------------------------------- |
| First run in `~/work/api`                                   | `api`                                          |
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

Per-project state and runtime paths carry a second component after `<project-id>`: `<target>`, **the named VM instance within a project**. A project has exactly **one target, named `default`**, in this version — there is no flag that selects another and no manifest key that declares one, so `<target>` is `default` in every path today.

The component exists in the paths anyway, deliberately. It is the one thing that cannot be added later without relocating every project's state: generations, volumes, and runtime files would all have to move. Reserving the segment now costs one directory level and keeps a future second VM per project (a variant with different resources, say) a purely additive change.

Two rules follow, and they are what keep the affordance honest:

- **State and runtime paths use `<target>` symmetrically.** `projects/<project-id>/<target>/` under the state root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)) and `vivarium/<project-id>/<target>/` under the runtime root ([`12-exec-and-shell.md`](./12-exec-and-shell.md)). A path that keyed one and not the other would silently cap the project at one running VM regardless of what the state layout allows.
- **Multiple sessions are not multiple targets.** Several `exec`/`shell` sessions attach to one target's VM; they never create one ([`12-exec-and-shell.md`](./12-exec-and-shell.md)).

## Teardown

`viv destroy` ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)) removes the project's state under `projects/<project-id>/`, clears its identity-index entry, **and** removes the `.vivarium/` marker, so the next `viv start` in that directory is a clean first run. The manifest binding is left untouched — `destroy` never removes it ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)).
