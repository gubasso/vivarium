# Project file

Record where the definition of the environment lives relative to the project it serves.[^read]

1. Create two projects needing different environments.
2. Configure each by the tool's documented mechanism.
3. Record where each definition is stored, and whether entering a project directory is enough to select the right one.

## vivarium

Yes: the manifest is a project's own file, and the binding from a project directory to it is recorded once and resolved on entry, so two projects needing different environments are two manifests and no argument at either start. The binding itself is machine-local state rather than a committed pointer, written only by an explicit `viv init --write` ([`02-config-and-xdg-layout.md`](../../spec/02-config-and-xdg-layout.md)), which is what keeps a project's tree free of a file that would mean nothing on another machine.

## flake-pilot

No: a registration writes `/usr/share/flakes/<app>.yaml`, plus an `<app>.d/` drop-in directory. The unit is the application, not the project — two projects wanting different environments for the same tool need two registered command names.

## glaipnir

No: one `glaipnir.conf`, in the checkout or under `$XDG_CONFIG_HOME`, per user rather than per project — and `_parse_conf` runs after the argument loop, so a value in the file overwrites the same value given on the command line.

## podman

Partial: a `Containerfile` can live in the project and describe the environment exactly, but nothing binds it to the directory or resolves it on entry — the binding is the user's shell history.

[^read]: Read at `flake-pilot` `main`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-18; `vivarium` `ceb0027` on 2026-08-20.
