# Boundary flag

A boundary is chosen once and used for months, on a host that changes in between. Three things can still reach that choice afterwards: a flag the user passes, a file the user did not write, or the host itself. This row asks the first, [the next row](./boundary-file.md) asks the second, and [Runs on a host without KVM](./no-kvm.md) asks the third.

1. Record which boundary the tool used on an ordinary first run.
2. Read its flag list and its call-time arguments for anything that selects a weaker one.
3. Pass it, and record which boundary the run then used, and what it said.

## vivarium

vivarium `ceb0027`, read 2026-08-19. Yes: one boundary and no second mode beneath it, so there is no flag to pass. A host that cannot provide it gets a refusal rather than a substitute, which is what the [separate-kernel rule](../../spec/08-invariants-and-guarantees.md) fixes.

## flake-pilot

flake-pilot `main`, read 2026-08-18. Yes: the engine is written into `/usr/share/flakes/<app>.yaml` at registration, and no call-time pseudo-argument revisits it. The set the pilot consumes — `@NAME`, `%remove`, `%interactive`, `%ignore_sync_error`, `%ignore_missing_volume_path`, `%progress`, `%port:number` — contains nothing that names an engine.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `--no-microvm` selects the container outright, in the user's own command. The stance is deliberate — a weaker sandbox beats no sandbox — and vivarium's [separate-kernel rule](../../spec/08-invariants-and-guarantees.md) takes the opposite position. Both are coherent; they disagree about what a sandbox is for.

## podman

podman 5.x, read 2026-08-19. No: the boundary is a per-invocation flag. Omit `--runtime krun` and the same command runs the same image under the default runtime, with no warning, and nothing records that the workload was meant to run behind a kernel of its own.
