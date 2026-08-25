# Boundary file

The companion to [the flag row](./boundary-flag.md). A flag is at least typed by the person who wanted it; a file is read by a run that never mentions it.[^read]

1. Read the tool's configuration schema for a key naming the engine, runtime, or boundary.
2. Record which files that key is read from, and who installs them.
3. Set it there rather than on the command line, re-run the ordinary invocation, and record which boundary was used.

## vivarium

Yes: there is no weaker boundary for a file to select, so no key in a manifest, an image, or a piece can name one. The [class-not-tool rule](../../spec/08-invariants-and-guarantees.md) is why the key does not exist rather than being refused — the backend is fixed by capability class, so there is nothing for a file to choose between.

## flake-pilot

No, and the `krun` route has a second lever the firecracker route does not.

Common to both: the drop-in directory `<app>.d/*.yaml` is read in alpha order with the last key winning, and rewrites an existing registration's options. A file dropped in beside a registration reaches what the registration fixed, and no merge stage reports that it did — the same mechanism that loses flake-pilot the [collision row](./collision.md).

At the `krun` route that reaches the boundary itself. The engine is an option rather than a binary: the registration carries `--opt "\--runtime=krun"`, so a drop-in that rewrites the option list can drop the workload onto `crun` and its shared kernel with nothing announcing the change. Upstream documents a second path to the same place — `runtime = "krun"` under `[engine]` in `/etc/containers/containers.conf` or its per-user counterpart — which sets the engine for every registration from outside every registration, and unsets it the same way. At the firecracker route the engine is the pilot binary the registered symlink points at, so a drop-in can weaken much about a registration but not which kernel it boots.

## glaipnir

Yes: the weaker boundary is reached by `--no-microvm` and by a failed host probe, and neither is a file. One adjacency is worth naming and is unverified at this revision: `_parse_conf` runs after the argument loop, so a key `glaipnir.conf` parses overwrites the same value given on the command line. Were the microVM selector ever among those keys, a file would not merely reach the boundary — it would outrank the flag.

## bunkerbox

No, by two files, and neither is the engine. Nothing names the engine — the Kata runtime is a literal in the source, which is why [the flag row](./boundary-flag.md) reads yes.

The first is `.bunkerbox/project.conf`, which bunkerbox generates itself on first run. Into it, it writes an auto-detected `passthrough` list — nine build-system detectors, so a repository with a `Cargo.toml` and a `Makefile` gets `cargo *` and `make *` — and leaves `profiles` empty. Both defaults matter together: the first opens the host-command channel, and the second is upstream's own condition for those commands running on the host with no sandbox at all. A first run therefore arrives at a weaker boundary than the one the image provides, through a file the user never opened.

The second is the runtime config the packager installs at `/usr/share/bunkerbox/<command>.conf`. It carries `network`, which may be `host`, and `workspace`, which may be `direct`; a project config may override the second and is forbidden from touching the first.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-19; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
