# Prebuilt image

Record what the first run has to do before the environment is usable.[^read]

1. On a machine that has never run the tool, run its first documented invocation.
2. Record whether it fetched a finished artifact or produced one, and what that cost.

## vivarium

No, by rule: the [pure-build rule](../../spec/08-invariants-and-guarantees.md) fixes the build as pure, a property an OCI registry pull cannot have since a tag is a mutable name. Not planned — the cheap cold start is a Nix binary cache, which delivers the same closure rather than a differently-built one.

## flake-pilot

Yes: `flake-ctl firecracker pull --kis-image <url>` fetches a finished tarball into the local image store, and the first run of a registered application boots it. Nothing is built on the machine at all.

The `krun` route is the same answer at podman's own path: the registration names a registry tag and the first call pulls it. Upstream publishes both stores, and neither route builds anything on the machine.

## glaipnir

Yes: the agent image starts `FROM` a published per-agent image on an OBS registry, so the first run pulls rather than builds — what is built locally is the thin layer `PACKAGES` and the hooks add.

## podman

Yes: the first `podman run` of an image not present pulls it, and that is the documented path.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `flake-pilot` `920f41e`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-20.
