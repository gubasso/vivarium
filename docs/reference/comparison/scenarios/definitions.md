# Definitions

Record how a user finds every sandbox that has been defined on the machine, running or not. Whether the running ones can be found is [the next row](./instances.md).[^read]

1. Define sandboxes for three different projects.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed and what is missing.

## vivarium

Yes: definitions live in one place — the config library's `manifests/` directory — and `viv manifest list` enumerates every one of them from any directory, each row naming the manifest, its path, its image, and its ordered pieces. `viv status -g` answers the narrower question beside it: which of those definitions have become sandboxes, running or resting. A manifest no command has ever started is configuration and appears only in the first listing, which is exactly the split this row asks about. `viv images list`, the shared-parts listing beside these, is still specified only.

## flake-pilot

Yes: `flake-ctl list --format table|json|csv` reports every registration — name, engine, config path. The unit is the application rather than the project, which is [Defined by a project file](./project-file.md#flake-pilot), but every unit that exists is listed.

This holds at both routes: `flake-ctl list` reports every registration whichever pilot it names. What it does not report is [what is running](./instances.md#flake-pilot).

## glaipnir

Yes: `status` reports the agents and images that exist, and the roster is fixed, so the set is small and fully known by construction. Enumeration is easy for the same reason [an agent outside the roster cannot run](./per-tool-credentials.md#glaipnir).

## bunkerbox

No: the CLI has a `list`, and it lists the tool's embedded YAML sequences rather than anything a user defined. Runtime configs are files under `/usr/share/bunkerbox` and project configs are files inside repositories; both are found with `ls` and `find`, which is the absence this row is asking about.

[^read]: Read at `vivarium` `e1b3c73` on 2026-08-26; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
