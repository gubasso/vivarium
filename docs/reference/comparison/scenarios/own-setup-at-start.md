# Own setup at start

The companion to [the build-time row](./own-setup-at-build.md). Some setup cannot happen at build: it needs the host as it is now, or the credential that arrived since. Record whether the tool has a place for it, and whether a second such step can be added without editing the first.

1. Write a setup step that must run each time the sandbox starts, before the user's work does.
2. Attach it by the tool's documented mechanism.
3. Attach a second one, and record what it took to add.

## vivarium

vivarium `ceb0027`, 2026-08-20. Yes, with no limit worth naming: a piece is a NixOS module, so a systemd service, a timer, or an activation step is declared the way it would be on any NixOS host, and it composes with every other layer through the same merge. A second one is a second module rather than an edit to the first, which is [the composition row](./composition.md#vivarium) paying out. What it is not is a shell hook — the unit is a declaration the module system can see, which is what lets [a collision be reported](./collision.md#vivarium).

## flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: `sci` runs the one `run=` command and then reboots, so the single execution slot is the application itself. That is the same in-guest emptiness that loses flake-pilot [a second session](./concurrent-sessions.md#flake-pilot).

## glaipnir

glaipnir `21ef389`, read 2026-08-20. Yes: `--run-hook` stages scripts into a mounted directory, and the entrypoint finds every `*.sh` there, sorts them, and runs each one on every start. The ordered `NN-*.sh` convention is what composes them, and each is validated with `shellcheck` before use. The cost is the same as the mechanism: they compose the way shell does, one after another, so nothing can report a disagreement between two of them — which is where [the collision row](./collision.md#glaipnir) reads `n/a`.

## podman

podman 5.x, 2026-08-20. Reachable, nothing arranges it: the start side is one command — `ENTRYPOINT` baked into the image, or `--entrypoint` replacing it for a single run — so setup means writing a script that does the work and then execs the real one. The mechanism is podman's and the arrangement is the user's: there is no directory of start steps to add to, so a second step edits the first, or rebuilds.
