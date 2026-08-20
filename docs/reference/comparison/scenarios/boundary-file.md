# Boundary file

The companion to [the flag row](./boundary-flag.md). A flag is at least typed by the person who wanted it; a file is read by a run that never mentions it.[^read]

1. Read the tool's configuration schema for a key naming the engine, runtime, or boundary.
2. Record which files that key is read from, and who installs them.
3. Set it there rather than on the command line, re-run the ordinary invocation, and record which boundary was used.

## vivarium

Yes: there is no weaker boundary for a file to select, so no key in a manifest, an image, or a piece can name one. The [class-not-tool rule](../../spec/08-invariants-and-guarantees.md) is why the key does not exist rather than being refused — the backend is fixed by capability class, so there is nothing for a file to choose between.

## flake-pilot

No: the drop-in directory `<app>.d/*.yaml` is read in alpha order with the last key winning, and rewrites an existing registration's options. A file dropped in beside a registration reaches what the registration fixed, and no merge stage reports that it did — the same mechanism that loses flake-pilot the [collision row](./collision.md).

## glaipnir

Yes: the weaker boundary is reached by `--no-microvm` and by a failed host probe, and neither is a file. One adjacency is worth naming and is unverified at this revision: `_parse_conf` runs after the argument loop, so a key `glaipnir.conf` parses overwrites the same value given on the command line. Were the microVM selector ever among those keys, a file would not merely reach the boundary — it would outrank the flag.

## podman

No: `containers.conf` sets the default OCI runtime, and both a system-wide copy and a per-user copy apply. An invocation that omits `--runtime` runs at whatever that file says, which is a boundary decided somewhere the command does not mention.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-19; `flake-pilot` `main` and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19.
