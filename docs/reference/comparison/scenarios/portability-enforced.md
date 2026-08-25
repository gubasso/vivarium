# Portability enforced

A unit that travels is not the same as a unit that works. Record what happens when a shared unit carries something only its author's machine has.[^read]

1. Put a literal personal path into a unit meant to be shared.
2. Hand it to a second user and build.
3. Record whether anything refused, warned, or noticed, and when.

## vivarium

Yes: a shared layer reaches the host only through portable variables — `${HOME}` and the four durable XDG directories — which every host resolves, and expansion happens at launch, so no expanded path is ever written into the artifact. A literal personal path in a shared artifact fails evaluation with `65` before the build, naming the artifact. This is the mechanism, not the advice: the same rule that makes the docs self-contained is the one the evaluator applies to a piece.

## flake-pilot

No: the firecracker registration names local file paths under `/var/lib/firecracker/images/`, and a drop-in carrying a host path carries it literally. Nothing checks, so the failure arrives on the colleague's machine at run time rather than on the author's at build time.

The `krun` route has a placeholder where the firecracker route has a literal — `%HOME/ai:%HOME/ai` resolves per user rather than baking one person's home directory in — and it is still not enforcement. Nothing checks a registration before it is written or before it is replayed, and [a `%VAR` with no matching variable becomes the literal name rather than failing](./environment.md#flake-pilot), so the mechanism that would carry portability is also the one that hides its absence.

## glaipnir

No: there is no shared unit for a check to apply to, and the hooks that can be copied by hand are shell scripts that may name anything. `shellcheck` validates them as shell, which is not the same question.

## bunkerbox

No, and the shareable unit is the one made of host paths. A profile's `bin` maps command names to absolute host locations and its `paths` list names host directories, so a toolchain outside the standard locations is a personal path by construction — upstream's own custom-profile example is `/opt/toolchain`, and the way a project config adopts one is by absolute path, illustrated in the documentation with a path under a named user's home. Nothing checks any of it. The first run on the colleague's machine is where it surfaces, as a missing binary rather than a message about a shared unit.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `main`, and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
