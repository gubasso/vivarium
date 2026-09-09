# ADR-0114: A lane runs where a place names it

## Context and Problem Statement

The suite selected tests by git stage, and one binary held forty-four trials needing three different host capabilities. Each trial probed the host and skipped itself on an unmet capability, unless `VIVARIUM_TEST_REQUIRE=1` inverted that. So a machine without `/dev/kvm` reported green over trials that never ran, and which trials ran was a property of the machine rather than of the configuration. On 2026-09-08 a GitHub runner turned out to carry `/dev/kvm`, twenty-six boot trials ran on a host with no systemd user manager, and CI failed.

## Considered Options

- Keep the runtime gate, with capability detection per place
- Name each lane for what it needs and let each place list the lanes it runs
- Mark the expensive trials `#[ignore]`

## Decision Outcome

Chosen option: `name each lane for what it needs` — where a test can run is a fact about the test, so it belongs in the name rather than in a probe the test runs on itself.

Five lanes: `unit` needs nothing, `local` the built binary, `eval` Nix, `net` the namespace tools, `boot` a guest. The need names the binary because nextest has no test attribute to filter on, so `.config/nextest.toml` selects each lane with `binary(<lane>_*)` and `Cargo.toml` requires the prefix. Each binary declares its need once, in `main`, and an unmet need fails its trials with the reason rather than skipping.

`.pre-commit-config.yaml` is the one place a lane's command is written. `just`, `git`, and CI reach a lane by hook id, replacing the justfile twins a comment asked a reader to keep byte-identical. `scripts/check-lane-register` holds those files to each other.

`VIVARIUM_TEST_REQUIRE` and `VIVARIUM_TEST_VIV` are deleted. No profile retries.

## Consequences

- Good: what ran is readable from the configuration, on any machine.
- Good: a lane a place cannot serve is named there, with its reason.
- Bad: `boot` gates the developer's push alone until a runner is shown to serve it.
- Bad: a boot trial written into another lane's file escapes the boot bound.

## Status

Implemented

Enacted in [`../../.config/nextest.toml`](../../.config/nextest.toml), [`../../.pre-commit-config.yaml`](../../.pre-commit-config.yaml), [`../../Cargo.toml`](../../Cargo.toml), [`../../justfile`](../../justfile), [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml), [`../../scripts/check-lane-register`](../../scripts/check-lane-register), [`../../tests/support/preflight.rs`](../../tests/support/preflight.rs), and the seven lane-named binaries under [`../../tests/`](../../tests/). Specified in [`../reference/testing-lanes.md`](../reference/testing-lanes.md).

Amends [`./ADR-0076-test-lanes-and-what-each-proves.md`](./ADR-0076-test-lanes-and-what-each-proves.md) — the "one runner profile each" it decided is now one profile per executable lane selected by binary name, rather than one per proof lane; the gated acceptance lane is a gate on a capable host rather than informational; and its consequence that every lane needs an ungated trial no longer holds, because nothing is ignored.

Amends [`./ADR-0101-the-check-surface-has-one-stage-per-claim.md`](./ADR-0101-the-check-surface-has-one-stage-per-claim.md) — one stage per claim still holds, and what changes is that a place names its claims by hook id: `just` and CI invoke hooks rather than carrying twin commands, and CI runs the push-stage claims as a matrix of named hooks rather than a second sweep.
