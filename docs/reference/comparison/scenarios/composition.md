# Composition

Record how a second concern is added to an environment that already has one. What happens when the two disagree is [the next row](./collision.md).[^read]

1. Define an environment carrying one concern.
2. Add a second concern written by somebody else, without editing the first.
3. Record what was edited to adopt it.

## vivarium

Yes: an image and an ordered list of pieces are imported as NixOS modules and merged by the module system, with no vivarium merge engine of its own. Lists concatenate, so every layer contributes to the package set, the mount list, and the egress allowlist without editing the layer beneath it.

## flake-pilot

Yes at both routes, at the registration: `<app>.d/*.yaml` drop-ins are read in alpha order, so a second author's file is adopted by dropping it in with nothing edited.

At the `krun` route there is a second level as well, and it is flake-pilot's own rather than the builder's. A registration names a `base_container` and an ordered `layers:` list, and at provisioning `podman-pilot` image-mounts the base, then each layer in list order, then the application container, syncing each onto the instance — the mechanism upstream describes as building a solution stack, base plus python plus python-app, and as delta containers pulled against a base that exists only once. A layer may also carry a file named `removed`, naming paths to be provisioned from the host rather than from the image, and that list accumulates across the layers. The costs are that provisioning needs root and escalates through `sudo` at launch, and that two layers writing one path are settled by list order with nothing reported, which is [the next row](./collision.md#flake-pilot).

At the firecracker route there is no such level: `firecracker-pilot` has neither key, so composition stops at the drop-in and the single KIS artifact underneath, and what goes into that artifact belongs to the builder that produced it.

## glaipnir

Yes: the extension surface is ordered `NN-*.sh` drop-in hooks, run as root at build and as `aiuser` on every start, `shellcheck`-validated before use, plus a `PACKAGES=(...)` array interpolated into the base install line. A second concern is a second file in the hooks directory.

## podman

No: a `Containerfile` composes linearly — one `FROM` and a sequence of steps, with multi-stage builds copying artifacts between stages. There is exactly one base, so two bases cannot be adopted together, and combining two authors' work means editing one file into the other by hand.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19; `flake-pilot` re-read at `920f41e` on 2026-08-22 for the `base_container` and `layers:` provisioning path.
