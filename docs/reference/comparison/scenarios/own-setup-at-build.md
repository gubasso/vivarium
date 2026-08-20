# Own setup at build

Record what a user can execute of their own while the environment is being produced. Read it beside [The build runs no user-supplied commands as root](./build-steps.md): the two ask one question from opposite sides, and a yes here is what a no there is refusing. What can run at each start is [the next row](./own-setup-at-start.md).

1. Write a setup step the tool does not provide — a repository added, a file generated.
2. Attach it so it runs while the environment is built.
3. Record whether it ran, as whom, and what a second person needs to reproduce the result.

## vivarium

vivarium `ceb0027`, 2026-08-20. No, by rule: there is no script slot at all, because arbitrary build steps running as root are refused. Build-time setup is expressed as declaration instead — a package to install, an option to set, or a derivation that produces the file. Most setup hooks convert; one that expects to reach the network mid-build does not, because that is the reproducibility [the same-definition row](./same-definition.md#vivarium) measures. Not planned, and the cost is real.

## flake-pilot

flake-pilot `44e3ab2`, read 2026-08-20. No: the registration flags carry no script. `--include-tar` and `--include-path` transfer a payload onto the instance rather than executing anything, and the image's own build happens in a toolchain the registration never sees.

## glaipnir

glaipnir `21ef389`, read 2026-08-20. Yes, and this is the subject that has it most directly: `--build-hook` runs the user's script as root inside the build context, which is how a package outside the default repositories gets its repository added — the mechanism [the packages row](./programs-installed.md#glaipnir) defers to. The cost is [the no-root-build row](./build-steps.md#glaipnir) and [the same-definition row](./same-definition.md#glaipnir), where the same generality reads as a loss.

## podman

podman 5.x, 2026-08-20. Yes: `RUN` covers the build side completely, as root, with no restriction on what it does. It is the widest build-time answer in the set, and what it costs is [the same-definition row](./same-definition.md#podman), where the same line names a package and the repository decides the version.
