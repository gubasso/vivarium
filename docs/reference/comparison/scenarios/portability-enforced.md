# Portability enforced

A unit that travels is not the same as a unit that works. Record what happens when a shared unit carries something only its author's machine has.

1. Put a literal personal path into a unit meant to be shared.
2. Hand it to a second user and build.
3. Record whether anything refused, warned, or noticed, and when.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: a shared layer reaches the host only through portable variables — `${HOME}` and the four durable XDG directories — which every host resolves, and expansion happens at launch, so no expanded path is ever written into the artifact. A literal personal path in a shared artifact fails evaluation with `65` before the build, naming the artifact. This is the mechanism, not the advice: the same rule that makes the docs self-contained is the one the evaluator applies to a piece.

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: the firecracker registration names local file paths under `/var/lib/firecracker/images/`, and a drop-in carrying a host path carries it literally. Nothing checks, so the failure arrives on the colleague's machine at run time rather than on the author's at build time.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: there is no shared unit for a check to apply to, and the hooks that can be copied by hand are shell scripts that may name anything. `shellcheck` validates them as shell, which is not the same question.

## podman

podman 5.x, 2026-08-19. No: nothing reads a `Containerfile` for a host path the colleague does not have, and a `-v` or `Volume=` line naming one is ordinary. The build succeeds and the run is what fails.
