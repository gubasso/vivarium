# Programs installed

Record how a user adds a program the base does not already have.[^read]

1. Pick a compiler or command line tool absent from the default environment.
2. Add it by the tool's documented mechanism.
3. Record where that request is written, and what a second person has to do to get the same set.

## vivarium

Yes: an image and a piece are NixOS modules, so a program is named in `environment.systemPackages` exactly as it would be on a NixOS host, and the module system concatenates those lists across every layer. Adding a piece therefore adds its packages without touching the manifest that imported it. The place to write it is deliberately not the manifest: [`03-artifact-model.md`](../../spec/03-artifact-model.md)'s key table is the manifest's whole surface and carries no package key, so a personal one-off goes through `extends = "./custom.nix"` and anything meant to be reused becomes a piece. That is the same split that makes the set shareable — the package a concern needs travels with the concern.

The request is an ordinary NixOS option inside the piece that needs it:

```nix
# pieces/rust-toolchain/default.nix
{ pkgs, ... }:
{ environment.systemPackages = [ pkgs.cargo-nextest ]; }
```

## flake-pilot

No: flake-pilot registers an image and never describes its contents, and no key in a registration or an `<app>.d/` drop-in names a package. `--include-tar` and `--include-path` come closest and are not the same thing: they copy a payload onto the instance at provisioning, so the user supplies built files rather than a name to resolve. A name does resolve one layer out, in the KIWI description the user [builds the image from](./guest-os.md#flake-pilot), and upstream ships one to copy — but that file belongs to KIWI, flake-pilot neither runs it nor records which description produced a registered image, so the set inside is not recoverable from anything the tool holds.

The request is a package name in an image description, resolved the next time the user rebuilds ([`appliance.kiwi`](https://github.com/OSInside/flake-pilot/blob/920f41e/appstore/firecracker/claude/appliance.kiwi)):

```xml
<package name="ripgrep"/>
```

## glaipnir

Yes: a `PACKAGES=(…)` array in the config file is interpolated into the image's `zypper install` line at build time. It resolves against the default Tumbleweed repositories only — a package from anywhere else needs a build hook that adds the repository first, which is the mechanism the next row measures.

The request is an array in the user's own config file, read at image build time:

```bash
PACKAGES=(ripgrep fd jq)
```

## podman

Yes: a `RUN` line in a `Containerfile`, which is the ordinary way and works. What it costs is the row below on pinning: the line names a package, the repository decides the version, and the same file built later produces a different set.

The request is a build step, and the repository rather than the line decides which version arrives:

```dockerfile
RUN zypper --non-interactive install ripgrep fd jq
```

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `920f41e`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-20.
