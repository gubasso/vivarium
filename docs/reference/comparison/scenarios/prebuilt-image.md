# Prebuilt image

Record what the first run has to do before the environment is usable.[^read]

1. On a machine that has never run the tool, run its first documented invocation.
2. Record whether it fetched a finished artifact or produced one, and what that cost.

## vivarium

No, by rule: the [pure-build rule](../../spec/08-invariants-and-guarantees.md) fixes the build as pure, a property an OCI registry pull cannot have since a tag is a mutable name. Not planned — the cheap cold start is a Nix binary cache, which delivers the same closure rather than a differently-built one.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18.
