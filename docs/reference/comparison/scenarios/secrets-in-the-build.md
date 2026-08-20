# Secrets in the build

Record whether the documented way of using the tool puts a credential into the artifact the environment is built from, and who can read it if one lands there. Whether anything stops a user putting one there anyway is [the next row](./secrets-enforced.md).

1. Read what the build consumes, and whether any documented flow puts a credential there.
2. Record what the tool says about it.
3. Record who else on the machine can read the artifact.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: a build-time secret is prohibited outright, and the reach of one is why the rule takes no exception. The store is shared read-only into every guest on the machine, so a secret in a store path is readable by every sandbox running there, including one deliberately running untrusted code. The rule is wider than "do not read a credential during the build": the manifest is itself compiled into a module and realised, so `[env] TOKEN = "…"` is a build-time secret whatever its launch-channel classification suggests. What replaces it is the agent channel above, a scoped short-lived value passed at launch, or an encrypted-at-rest scheme the user composes in — vivarium performs no decryption and holds no identity.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: the stance is stated up front and holds in the code — nothing is baked into the image, authentication happens at runtime inside the container, the token lands in a host cache directory the user owns, and the image carries the label `security.credentials="runtime-only"`. Whether anything holds a user to it is [the next row](./secrets-enforced.md#glaipnir), and there the answer changes.
