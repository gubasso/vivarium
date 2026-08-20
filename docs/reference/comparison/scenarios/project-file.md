# Project file

Record where the definition of the environment lives relative to the project it serves.

1. Create two projects needing different environments.
2. Configure each by the tool's documented mechanism.
3. Record where each definition is stored, and whether entering a project directory is enough to select the right one.

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: a registration writes `/usr/share/flakes/<app>.yaml`, plus an `<app>.d/` drop-in directory. The unit is the application, not the project — two projects wanting different environments for the same tool need two registered command names.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: one `glaipnir.conf`, in the checkout or under `$XDG_CONFIG_HOME`, per user rather than per project — and `_parse_conf` runs after the argument loop, so a value in the file overwrites the same value given on the command line.

## podman

podman 5.x, 2026-08-18. Partial: a `Containerfile` can live in the project and describe the environment exactly, but nothing binds it to the directory or resolves it on entry — the binding is the user's shell history.
