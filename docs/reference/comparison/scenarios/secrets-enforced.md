# Secrets enforced

The companion to [the row above](./secrets-in-the-build.md). A documented flow that keeps credentials out of the artifact is worth having; this row asks what happens to the user who ignores it.[^read]

1. Put a credential into the build by whatever route the tool leaves open.
2. Record whether anything refuses, warns, or notices.
3. Record whether what noticed is a rule, a check, or a convention.

## vivarium

Partial: the prohibition is a binding rule rather than a practice, and the pure build means there is no arbitrary build step to smuggle one through. What detection cannot be is complete, and the specification says so itself — a plaintext secret is not decidable by inspection, so the `manifest-no-inline-secret` check warns heuristically, and a value it does not flag is not a promise. A rule that binds and a check that only warns is two thirds of an answer.

## flake-pilot

No: nothing reads an image before it is registered, and nothing reads an `--include-path` payload before it is synced onto the instance. There is no check to bypass because there is no stage that inspects — a credential in either place is registered and used exactly like anything else.

Neither route inspects anything: the `krun` route adds an image and a mount to the ways material arrives, and no stage reads either.

## glaipnir

No: it is practice rather than a rule. Build hooks run arbitrary commands as root at build time, so a user who puts a credential there gets it in the image, and nothing objects — no check reads the hooks, and the `security.credentials="runtime-only"` label keeps saying what it said.

## podman

No: the safe mechanism of [the row above](./secrets-in-the-build.md#podman) is opt-in and its absence is silent. Nothing reads a `Containerfile` for a credential, no warning distinguishes `--mount=type=secret` from a `COPY` of the same file, and the layer that carries it is as pullable as any other.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `flake-pilot` `44e3ab2` and `podman` 5.x on 2026-08-20.
