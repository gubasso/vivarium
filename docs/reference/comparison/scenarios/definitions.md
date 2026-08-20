# Definitions

Record how a user finds every sandbox that has been defined on the machine, running or not. Whether the running ones can be found is [the next row](./instances.md).[^read]

1. Define sandboxes for three different projects.
2. Run the tool's enumeration command from an unrelated directory.
3. Record what is listed and what is missing.

## vivarium

Specified, not built: there is no machine-wide index of projects today. `viv status` reports the current project, and `viv status -g` together with `viv images list` are specified and do not run — the `*`. A definition lives in the project directory it belongs to, so the filesystem holds the answer and nothing collects it.

## flake-pilot

Yes: `flake-ctl list --format table|json|csv` reports every registration — name, engine, config path. The unit is the application rather than the project, which is [Defined by a project file](./project-file.md#flake-pilot), but every unit that exists is listed.

## glaipnir

Yes: `status` reports the agents and images that exist, and the roster is fixed, so the set is small and fully known by construction. Enumeration is easy for the same reason [an unknown tool cannot run](./any-tool.md#glaipnir).

## podman

Yes: `podman images` lists every image on the machine, from any directory.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
