# 02 — Config and XDG layout

vivarium stores all state under standard per-user XDG directories, split by durability. The governing
rule and rationale are in
[`../../decisions/ADR-0005-xdg-user-config-layout.md`](../../decisions/ADR-0005-xdg-user-config-layout.md):
**config is authored, cache is derived, state is runtime, data is pinned inputs.**

## The four roots

- **Config root** (`$XDG_CONFIG_HOME/vivarium/`) — the user's source of truth. Holds the global
  config file, the `images/` library, the `pieces/` library, and the `manifests/` library. Everything
  here is hand-authored and may be version-controlled by the user. **vivarium only reads the config
  root; it never writes, creates, or scaffolds anything here** (N13 in
  [`08-invariants-and-guarantees.md`](08-invariants-and-guarantees.md) — config is read-only to the
  tool). The tool's own writes go to state, data, or cache only.
- **Data root** (`$XDG_DATA_HOME/vivarium/`) — pinned external module libraries pulled in as inputs.
- **State root** (`$XDG_STATE_HOME/vivarium/`) — per-project runtime state the tool writes: the built
  VM's store output reference, a stable VM identity that survives restarts, the **project registry**
  (the project→manifest binding — see below), and logs.
- **Cache root** (`$XDG_CACHE_HOME/vivarium/`) — derived, regenerable artifacts: Nix evaluation cache
  and built VM images. Safe to delete; the tool rebuilds it.

The config, data, state, and cache roots hold, respectively, what the user edits, what is pinned as
input, what a run produces, and what can be rebuilt. Any new artifact is placed by asking which of
those four it is — and because config is read-only to the tool, anything the tool must write is by
definition state, data, or cache, never config.

## Global config file

The config root holds one global config file (TOML) carrying user-wide defaults. It is hand-authored
and read-only to the tool. It does **not** hold the project→manifest binding: that binding is
machine-specific, non-portable runtime state, so it lives in the state root, not here.

## Project registry (state)

The **project registry** is the single home for project→manifest bindings: a map from a project
directory to the manifest it resolves to. It lives under the **state root** because it is
machine-local, tool-managed, and not portable — a record of what the tool has bound on this machine,
keyed by the project's absolute path. The tool writes it only on an explicit, user-directed action
(`viv init --write`, see [`01-command-surface.md`](01-command-surface.md)), never as a side effect of
a normal command.

There are no per-project binding files inside the repository. vivarium writes nothing into a project's
own tree (N9); a project is bound by a registry entry, not by a committed or gitignored pointer.

## Resolution precedence

The effective manifest is resolved highest-wins, per
[`../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../decisions/ADR-0011-config-read-only-binding-in-state.md):

1. `--manifest` command-line flag — a single-invocation override, never persisted.
2. `VIVARIUM_MANIFEST` environment variable — a runtime override, never persisted.
3. Project-registry entry (in the state root) for the project's path.
4. Otherwise, **fail closed** with a copy-pasteable snippet to bind the project.

## The libraries

`images/`, `pieces/`, and `manifests/` under the config root hold the composable artifacts. Their
shapes are specified in [`03-artifact-model.md`](03-artifact-model.md). Images and pieces are Nix
modules; manifests are TOML that the tool compiles, per
[`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md).
