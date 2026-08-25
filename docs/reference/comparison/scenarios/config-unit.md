# Config unit

Record what a user can hand a colleague so the colleague's environment gains one concern — and whether that unit is the concern or the whole environment. Whether it then works unchanged is [the next row](./portability-enforced.md).[^read]

1. Configure one concern that needs a package and a host path.
2. Identify the smallest unit carrying it.
3. Record what the colleague receives, and what else comes with it.

## vivarium

Yes: images and pieces are the shared class and the manifest is the personal one. A piece carries its whole concern — packages, guest config, mounts, environment, and any third-party flake input it needs through its own `inputs.toml` — so adopting it is one name in the colleague's `pieces` list. The same split is how a team makes a guarantee unwaivable: a piece setting a value with `mkForce` outranks every personal manifest.

## flake-pilot

Yes: an `<app>.d/*.yaml` drop-in is smaller than the registration and carries one concern's options, and a colleague adopts it by copying it into place. That holds at both routes, the drop-in being a property of the registration rather than of the engine.

At the `krun` route there is a second unit below the whole environment. A delta container is an image carrying just its own concern, adopted by naming it in [the registration's `layers:` list](./composition.md#flake-pilot) against a base that exists once — which is upstream's own stated reason for the mechanism, that only small deltas need pulling. At the firecracker route the image is the other unit and it is the whole environment, so the drop-in is the only small one there.

## glaipnir

No: configuration is one `glaipnir.conf`, in the checkout or under `$XDG_CONFIG_HOME`, per user rather than per concern — there is no unit smaller than that file, and it is the user's own machine-local settings. The hooks directory can be copied by hand, which is file transfer rather than adoption.

## bunkerbox

Yes: a sandbox profile is one file carrying one concern — the binaries, paths, and environment one toolchain needs — and a colleague adopts it by dropping it somewhere and naming its absolute path. It carries no image, no tool, and no runtime policy with it. Whether it then works on their machine is [the next row](./portability-enforced.md#bunkerbox).

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `flake-pilot` re-read at `920f41e` on 2026-08-22 for the delta container as a unit smaller than the environment; `bunkerbox` `b7f14f3` on 2026-08-25.
