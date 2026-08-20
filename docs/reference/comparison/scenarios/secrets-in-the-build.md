# Secrets in the build

Record whether the documented way of using the tool puts a credential into the artifact the environment is built from, and who can read it if one lands there. Whether anything stops a user putting one there anyway is [the next row](./secrets-enforced.md).[^read]

1. Read what the build consumes, and whether any documented flow puts a credential there.
2. Record what the tool says about it.
3. Record who else on the machine can read the artifact.

## vivarium

Yes: a build-time secret is prohibited outright, and the reach of one is why the rule takes no exception. The store is shared read-only into every guest on the machine, so a secret in a store path is readable by every sandbox running there, including one deliberately running untrusted code. The rule is wider than "do not read a credential during the build": the manifest is itself compiled into a module and realised, so `[env] TOKEN = "…"` is a build-time secret whatever its launch-channel classification suggests. What replaces it is the agent channel above, a scoped short-lived value passed at launch, or an encrypted-at-rest scheme the user composes in — vivarium performs no decryption and holds no identity.

## flake-pilot

No: there is no secrets mechanism to use, so credential material reaches the guest the only ways anything does — written into the image, or copied by `--include-path` / `--include-tar` at provisioning — and both carry it in clear inside the artifact that gets registered and shared. [The shipping row](./shipping-a-secret.md#flake-pilot) records the absence itself: the word does not appear in the repository outside a CI workflow's own credentials.

## glaipnir

Yes: the stance is stated up front and holds in the code — nothing is baked into the image, authentication happens at runtime inside the container, the token lands in a host cache directory the user owns, and the image carries the label `security.credentials="runtime-only"`. Whether anything holds a user to it is [the next row](./secrets-enforced.md#glaipnir), and there the answer changes.

## podman

Reachable, nothing arranges it: `podman build --secret id=…,src=…` mounts a credential at `/run/secrets/<id>` for one `RUN --mount=type=secret` step, and upstream states it will "not end up stored in the final image, or be seen in other stages". That is a real answer and it is the one you have to know about — `ENV TOKEN=…`, a `COPY` of a credential file, or a token on a `RUN` line are all ordinary, all build the image, and all leave it there for anyone who can read the layer. Whether anything catches that is [the next row](./secrets-enforced.md#podman).

The safe form is the longer one, and nothing pushes a user toward it:

```dockerfile
RUN --mount=type=secret,id=npmrc,target=/root/.npmrc npm install
```

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `flake-pilot` `44e3ab2` on 2026-08-20, for the absence [the shipping row](./shipping-a-secret.md#flake-pilot) read at that revision; `podman` 5.x on 2026-08-20, from the `podman-build` manual page.
