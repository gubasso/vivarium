# Programs installed

Record how a user adds a program the base does not already have.

1. Pick a compiler or command line tool absent from the default environment.
2. Add it by the tool's documented mechanism.
3. Record where that request is written, and what a second person has to do to get the same set.

## vivarium

vivarium `ceb0027`, 2026-08-20. Yes: an image and a piece are NixOS modules, so a program is named in `environment.systemPackages` exactly as it would be on a NixOS host, and the module system concatenates those lists across every layer. Adding a piece therefore adds its packages without touching the manifest that imported it. The place to write it is deliberately not the manifest: [`03-artifact-model.md`](../../spec/03-artifact-model.md)'s key table is the manifest's whole surface and carries no package key, so a personal one-off goes through `extends = "./custom.nix"` and anything meant to be reused becomes a piece. That is the same split that makes the set shareable — the package a concern needs travels with the concern.

## flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: flake-pilot registers an image and never describes its contents. Upstream is explicit that images are built by any means the user likes — KIWI, podman, mkosi, OBS, koji — and no key in a registration or an `<app>.d/` drop-in names a package. `--include-tar` and `--include-path` come closest and are not the same thing: they copy a payload onto the instance at provisioning, so the user supplies built files rather than a name to resolve.

## glaipnir

glaipnir `21ef389`, read 2026-08-20. Yes: a `PACKAGES=(…)` array in the config file is interpolated into the image's `zypper install` line at build time. It resolves against the default Tumbleweed repositories only — a package from anywhere else needs a build hook that adds the repository first, which is the mechanism the next row measures.

## podman

podman 5.x, 2026-08-20. Yes: a `RUN` line in a `Containerfile`, which is the ordinary way and works. What it costs is the row below on pinning: the line names a package, the repository decides the version, and the same file built later produces a different set.
