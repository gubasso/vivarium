# Composition

Record how a second concern is added to an environment that already has one. What happens when the two disagree is [the next row](./collision.md).[^read]

1. Define an environment carrying one concern.
2. Add a second concern written by somebody else, without editing the first.
3. Record what was edited to adopt it.

## vivarium

Yes: an image and an ordered list of pieces are imported as NixOS modules and merged by the module system, with no vivarium merge engine of its own. Lists concatenate, so every layer contributes to the package set, the mount list, and the egress allowlist without editing the layer beneath it.

## flake-pilot

Yes, at the registration: `<app>.d/*.yaml` drop-ins are read in alpha order, so a second author's file is adopted by dropping it in with nothing edited. Inside the guest there is no second level — the image is one artifact, and composing what goes into it belongs to the builder that produced it rather than to flake-pilot.

Both routes compose the same way and stop at the same place, because `<app>.d/` belongs to the registration. What differs is only what the one artifact underneath is: a KIS tarball at the firecracker route, a container image at the `krun` route.

## glaipnir

Yes: the extension surface is ordered `NN-*.sh` drop-in hooks, run as root at build and as `aiuser` on every start, `shellcheck`-validated before use, plus a `PACKAGES=(...)` array interpolated into the base install line. A second concern is a second file in the hooks directory.

## podman

No: a `Containerfile` composes linearly — one `FROM` and a sequence of steps, with multi-stage builds copying artifacts between stages. There is exactly one base, so two bases cannot be adopted together, and combining two authors' work means editing one file into the other by hand.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
