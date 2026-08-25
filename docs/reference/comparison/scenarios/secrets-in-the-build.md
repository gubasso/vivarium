# Secrets in the build

Record whether the documented way of using the tool puts a credential into the artifact the environment is built from, and who can read it if one lands there. Whether anything stops a user putting one there anyway is [the next row](./secrets-enforced.md).[^read]

1. Read what the build consumes, and whether any documented flow puts a credential there.
2. Record what the tool says about it.
3. Record who else on the machine can read the artifact.

## vivarium

Yes: a build-time secret is prohibited outright, and the reach of one is why the rule takes no exception. The store is shared read-only into every guest on the machine, so a secret in a store path is readable by every sandbox running there, including one deliberately running untrusted code. The rule is wider than "do not read a credential during the build": the manifest is itself compiled into a module and realised, so `[env] TOKEN = "…"` is a build-time secret whatever its launch-channel classification suggests. What replaces it is the agent channel above, a scoped short-lived value passed at launch, or an encrypted-at-rest scheme the user composes in — vivarium performs no decryption and holds no identity.

## flake-pilot

Yes at both routes, and not because a mechanism keeps a secret out — because there is no build here to put one in. flake-pilot registers an image it did not make: `flake-ctl firecracker pull` fetches a prebuilt KIS archive into a local store, and a podman registration names an image in a registry. Neither pilot builds nor commits one, so the artifact a registration refers to is upstream of the tool, and a credential inside it is the image builder's doing rather than a flow this subject documents.

What a registration can carry is an `include.tar` / `include.path` payload, and that lands on the instance rather than in the image. At the firecracker route it is synced into a per-instance ext2 overlay mounted as the upper layer of an overlay whose lower layer is the image, then unmounted before boot; at the `krun` route it is synced into `podman mount <container-id>`, the created container's own writable rootfs. Credential material a registration needs therefore ends up where [glaipnir's stated stance](#glaipnir) puts it — authenticated at run time, persisted in instance storage, absent from the artifact a second person receives.

The absence this subject does have is a secrets mechanism, and it is scored where it belongs rather than twice here: nothing inspects what a payload carries ([the next row](./secrets-enforced.md#flake-pilot)), and nothing lets a secret travel with the definition ([the shipping row](./shipping-a-secret.md#flake-pilot)), the word being absent from the repository outside a CI workflow's own credentials. This row asks only whether the documented flow puts one in the artifact, and it does not.

## glaipnir

Yes: the stance is stated up front and holds in the code — nothing is baked into the image, authentication happens at runtime inside the container, the token lands in a host cache directory the user owns, and the image carries the label `security.credentials="runtime-only"`. Whether anything holds a user to it is [the next row](./secrets-enforced.md#glaipnir), and there the answer changes.

## bunkerbox

Yes: the documented credential flow is a runtime one. An image config's `encrypt` list names files inside the tool's persisted home, sealed between runs and never present at build time, so nothing in the flow a user is taught puts a credential into the archive. The build itself is an ordinary container build over a `containerfile` with `build_args`, so a credential put there anyway is in the image — and the archive a package installs under `/usr/share/bunkerbox/oci/` is a world-readable file on every machine that installs it. Whether anything catches that is [the next row](./secrets-enforced.md#bunkerbox).

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `flake-pilot` `44e3ab2` on 2026-08-20, for the absence [the shipping row](./shipping-a-secret.md#flake-pilot) read at that revision, and re-read at `920f41e` on 2026-08-22 for where an include payload lands and for the absence of any build step; `bunkerbox` `b7f14f3` on 2026-08-25.
