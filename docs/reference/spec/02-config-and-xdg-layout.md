# 02 — Config and XDG layout

nixvault stores all state under standard per-user XDG directories, split by durability. The governing
rule and rationale are in
[`../../decisions/ADR-0005-xdg-user-config-layout.md`](../../decisions/ADR-0005-xdg-user-config-layout.md):
**config is authored, cache is derived, state is runtime, data is pinned inputs.**

## The four roots

- **Config root** (`$XDG_CONFIG_HOME/nixvault/`) — the user's source of truth. Holds the global
  config file, the `images/` library, the `pieces/` library, and the `manifests/` library. Everything
  here is hand-authored and may be version-controlled by the user.
- **Data root** (`$XDG_DATA_HOME/nixvault/`) — pinned external module libraries pulled in as inputs.
- **State root** (`$XDG_STATE_HOME/nixvault/`) — per-project runtime state: the built VM's store
  output reference, a stable VM identity that survives restarts, and logs.
- **Cache root** (`$XDG_CACHE_HOME/nixvault/`) — derived, regenerable artifacts: Nix evaluation cache
  and built VM images. Safe to delete; the tool rebuilds it.

The config, data, state, and cache roots hold, respectively, what the user edits, what is pinned as
input, what a run produces, and what can be rebuilt. Any new artifact is placed by asking which of
those four it is.

## Global config file

The config root holds one global config file (TOML). It carries user-wide defaults and the optional
**project registry** — a list of entries mapping a project directory to a manifest, plus an optional
default manifest for unregistered projects. The registry is what lets a project bind to a manifest
without any file inside the repository.

## Per-project binding files

A project may instead name its manifest from within the repository:

- **Committed pointer** (`.nixvault.toml`) — names the manifest and is intended to be committed and
  shared with a team.
- **Personal override** (`.nixvault.local.toml`) — names a manifest for one user on one machine and
  is gitignored; it overrides the committed pointer.

## Resolution precedence

The effective manifest is resolved highest-wins, per
[`../../decisions/ADR-0006-manifest-binding-and-precedence.md`](../../decisions/ADR-0006-manifest-binding-and-precedence.md):

1. `--manifest` command-line flag.
2. Environment variable override.
3. Repository personal override (`.nixvault.local.toml`).
4. Repository committed pointer (`.nixvault.toml`).
5. Project-registry entry in the global config file.
6. Configured default manifest.
7. Otherwise, **fail closed**.

## The libraries

`images/`, `pieces/`, and `manifests/` under the config root hold the composable artifacts. Their
shapes are specified in [`03-artifact-model.md`](03-artifact-model.md). Images and pieces are Nix
modules; manifests are TOML that the tool compiles, per
[`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md).
