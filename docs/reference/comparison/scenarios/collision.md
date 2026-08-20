# Collision

Composition is cheap until two parts set the same thing. Record what the tool does then: report it, resolve it silently, or have nothing to resolve.

1. Adopt two parts that set the same value to different things.
2. Build, and record what the tool said.
3. Record which value the guest ended up with, and whether anything named the other.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: scalars resolve by priority rather than by position — a shared piece proposes with `mkDefault` and the user's manifest outranks it, a policy floor uses `mkForce` and nothing outranks that, and two definitions surviving at the same priority fail evaluation with `65` rather than being settled by order. That last rule is what lets two independently written pieces be adopted together: a collision is reported, never resolved behind the user's back. `viv config eval` and `viv config sources` give the merged view with provenance, so the answer to "who set this" is a command rather than a reading exercise.

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: `<app>.d/*.yaml` is read in alpha order and the last key wins. Two drop-ins setting one key is neither a conflict nor a merge — the later filename wins — so adopting a second author's file can undo the first's without saying so, and the deciding fact is a filename. The same mechanism is what loses flake-pilot the [boundary-file row](./boundary-file.md#flake-pilot).

## glaipnir

glaipnir `21ef389`, read 2026-08-18. n/a: hooks compose the way shell does, by running one after another, and `PACKAGES=(...)` is one array in one file. There is no declaration for two parts to disagree over, so there is no stage at which a disagreement could be reported. Not a gap — a different shape of extension, whose cost is paid in [the row above](./composition.md#glaipnir) rather than here.

## podman

podman 5.x, 2026-08-19. n/a: there is one `Containerfile` and one author of it at a time, so two parts never meet to collide. A later `RUN` overwriting an earlier one's work is a script overwriting itself, which is the ordinary reading of a sequence.
