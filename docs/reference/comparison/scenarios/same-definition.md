# Same definition

Record whether a definition produces the same environment on a second machine and at a later date, and what a second person has to be given for that to hold.[^read]

1. Record the definition and every version it names.
2. Reproduce the environment from it on a second machine, a month later.
3. Record what identifies each result, whether the two are the same object or merely similar, and what fixed each version.

## vivarium

Yes, by two mechanisms that need each other. The [pure-build rule](../../spec/08-invariants-and-guarantees.md) fixes the build as pure — the same manifest closure and lock give the same store output anywhere — and exactly one lockfile is in force, pinning the whole input graph rather than a revision here and there. By default that is the per-target lock vivarium owns under the data root, created by the first evaluation and moved only by `viv update`; a team that wants one shared answer puts a read-only override lock beside the manifest, and it then outranks the tool-owned lock for everybody. `viv config` reports which of the two is in force. A piece declaring a flake input the lock has no node for fails closed with `78`, naming the input and the artifact, rather than resolving it quietly — an unannounced input jump is exactly what the pin exists to prevent.

## flake-pilot

At the firecracker route, partial. The unit is a versioned tarball fetched by URL into `/var/lib/firecracker/images/<name>/`, and what this row asks is what a second person, on their own machine a month later, ends up with from what they were handed. There are two things to hand them, and neither closes the gap.

One is the tarball URL, which fetches bytes from a place. KIWI writes a sha256 record into the KIS archive and `pull` verifies the unpacked rootfs against it, which establishes that the download arrived whole rather than which tarball was meant — a rebuild published at the same URL carries its own matching record. An identifier outside the archive does now exist: upstream began publishing a `<image>.tar.xz.sha256` beside each appstore tarball at [`36090e6`](https://github.com/OSInside/flake-pilot/commit/36090e6db7e244494982303f47cbb8453b9395cf) on 2026-08-23, after this subject's reading. `pull` never fetches it, so it is a value a second person checks by hand rather than a name the tool holds either party to.

The other is the description, and it is published: [`appstore/firecracker/claude/`](https://github.com/OSInside/flake-pilot/tree/920f41e/appstore/firecracker/claude) and [`appstore/podman/claude/`](https://github.com/OSInside/flake-pilot/tree/920f41e/appstore/podman/claude) each carry an `appliance.kiwi` and a `config.sh`, which [the guest OS row](./guest-os.md#flake-pilot) and [the setup row](./own-setup-at-build.md#flake-pilot) already credit. What it does not do is fix an environment. Its repositories are moving branches, its packages carry no version, and `config.sh` installs the agent with `npm install -g` and pipes an installer from `sdk.cloud.google.com` into a shell, so a rebuild a month later is a different image built honestly from the same file. The KIS archive does not carry the description either, so the artifact cannot settle what the recipe leaves open.

Upstream states the position rather than leaving it implicit: the builder is the user's choice, and trust is anchored at the image source, which is what the https-only fetch and its documented escape hatch express. Published is not reproduced, and that gap is the partial.

At the `krun` route, no: the unit the registration names is `public.ecr.aws/b9k1j9y6/ai/claude:latest`, a moving tag on a registry publishing nightly builds. Two colleagues registering the same command on two days hold two different environments and nothing tells either of them so. The exact answer exists at this engine — [an image referenced by digest is one artifact](#podman) — and the registration upstream publishes is the one nobody is pointed at. This is the row where the two routes separate most sharply, because the firecracker route reaches its partial by having no mechanism for moving at all.

## glaipnir

No: `Containerfile.agent` starts `FROM` a per-agent image at `:latest`, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice, and there is nothing to hand a second user that fixes any of them.

## podman

Reachable, nothing arranges it: an image referenced by digest is exactly one artifact, and a colleague given the digest gets it. The published form is a tag, the documented flow is a tag, and a `Containerfile` rebuilt a month later re-executes its `RUN` steps against whatever the network serves that day. The exact answer exists and is the one nobody is pointed at.

[^read]: Read at `vivarium` `ceb0027`, `glaipnir` `21ef389`, and `podman` 5.x on 2026-08-18; `flake-pilot` `920f41e` on 2026-08-20, re-read at the same revision on 2026-08-22 for where upstream anchors trust in an image, and the KIS checksum path re-read at `main` on 2026-08-24, where a digest published beside each appstore tarball had appeared since that revision. The appstore descriptions behind both routes were read at `920f41e` the same day.
