# Secrets enforced

The companion to [the row above](./secrets-in-the-build.md). A documented flow that keeps credentials out of the artifact is worth having; this row asks what happens to the user who ignores it.[^read]

1. Put a credential into the build by whatever route the tool leaves open.
2. Record whether anything refuses, warns, or notices.
3. Record whether what noticed is a rule, a check, or a convention.

## vivarium

Partial: the prohibition is a binding rule rather than a practice, and the pure build means there is no arbitrary build step to smuggle one through. What detection cannot be is complete, and the specification says so itself — a plaintext secret is not decidable by inspection, so the `manifest-no-inline-secret` check warns heuristically, and a value it does not flag is not a promise. A rule that binds and a check that only warns is two thirds of an answer.

## glaipnir

No: it is practice rather than a rule. Build hooks run arbitrary commands as root at build time, so a user who puts a credential there gets it in the image, and nothing objects — no check reads the hooks, and the `security.credentials="runtime-only"` label keeps saying what it said.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18.
