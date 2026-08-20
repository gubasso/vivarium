# Composition

Record how a second concern is added to an environment that already has one. What happens when the two disagree is [the next row](./collision.md).

1. Define an environment carrying one concern.
2. Add a second concern written by somebody else, without editing the first.
3. Record what was edited to adopt it.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: an image and an ordered list of pieces are imported as NixOS modules and merged by the module system, with no vivarium merge engine of its own. Lists concatenate, so every layer contributes to the package set, the mount list, and the egress allowlist without editing the layer beneath it.

## flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: two mechanisms, at two levels. An image composes by OCI layering, `--base` for a delta container and a repeatable ordered `--layer`; a registration composes by drop-in, `<app>.d/*.yaml` read in alpha order. A second author's file is adopted by dropping it in, with nothing edited.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the extension surface is ordered `NN-*.sh` drop-in hooks, run as root at build and as `aiuser` on every start, `shellcheck`-validated before use, plus a `PACKAGES=(...)` array interpolated into the base install line. A second concern is a second file in the hooks directory.

## podman

podman 5.x, 2026-08-19. No: a `Containerfile` composes linearly — one `FROM` and a sequence of steps, with multi-stage builds copying artifacts between stages. There is exactly one base, so two bases cannot be adopted together, and combining two authors' work means editing one file into the other by hand.
