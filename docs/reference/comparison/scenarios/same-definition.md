# Same definition

Record whether a definition produces the same environment on a second machine and at a later date, and what a second person has to be given for that to hold.

1. Record the definition and every version it names.
2. Reproduce the environment from it on a second machine, a month later.
3. Record what identifies each result, whether the two are the same object or merely similar, and what fixed each version.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes, by two mechanisms that need each other. The [pure-build rule](../../spec/08-invariants-and-guarantees.md) fixes the build as pure — the same manifest closure and lock give the same store output anywhere — and exactly one lockfile is in force, pinning the whole input graph rather than a revision here and there. By default that is the per-target lock vivarium owns under the data root, created by the first evaluation and moved only by `viv update`; a team that wants one shared answer puts a read-only override lock beside the manifest, and it then outranks the tool-owned lock for everybody. `viv config` reports which of the two is in force. A piece declaring a flake input the lock has no node for fails closed with `78`, naming the input and the artifact, rather than resolving it quietly — an unannounced input jump is exactly what the pin exists to prevent.

## flake-pilot

flake-pilot `main`, read 2026-08-18. Partial: the firecracker unit is a versioned tarball fetched by URL into `/var/lib/firecracker/images/<name>/`, so a second user given the same URL has so far received the same rootfs. Nothing makes that a property rather than a habit — no digest, and the definition the image was built from is not carried, so there is nothing to rebuild from and nothing to compare against.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. No: `Containerfile.agent` starts `FROM` a per-agent image at `:latest`, then installs whatever `PACKAGES=(...)` names and runs whatever build hooks were given. Two of those three inputs move without notice, and there is nothing to hand a second user that fixes any of them.

## podman

podman 5.x, read 2026-08-18. Reachable, nothing arranges it: an image referenced by digest is exactly one artifact, and a colleague given the digest gets it. The published form is a tag, the documented flow is a tag, and a `Containerfile` rebuilt a month later re-executes its `RUN` steps against whatever the network serves that day. The exact answer exists and is the one nobody is pointed at.
