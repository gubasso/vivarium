# ADR-0115: The guest agent is a binary of the crate

## Context and Problem Statement

The binary embeds the source that the guest-agent derivation builds from, and that source was a nested workspace member, `crates/vivarium-guest-agent`. Cargo never puts a nested package inside its parent's tarball, so `cargo package` failed at verification, and a published crate would fail `build.rs` for every user who ran `cargo install vivarium`. `Q-039` recorded three ways out.

## Considered Options

- Do not publish the crate, and ship through cargo-dist's installers and the Nix flake alone.
- Publish the agent as its own crate, and have the guest derivation fetch it from crates.io by version.
- Keep the embedding and remove the nesting: the agent becomes a binary target of the `vivarium` package, behind a `guest` feature.
- Make the agent a subcommand of `viv`.

## Decision Outcome

Chosen option: `a binary target behind a guest feature`, because it keeps one package and one embedded tree, and `cargo package` then verifies.

A fetch replaces the embedded copy with a download, which is what the embedding exists to avoid. Not publishing gives up `cargo install vivarium`. A subcommand saves no build, because the guest compiles its own copy of the agent either way, and it adds the host-only dependencies to every guest image, its build time, and its attack surface.

The `guest` feature holds the agent's own dependencies. `default` stays `host`, so `cargo install vivarium` installs `viv` and `vivarium-supervisor` and no agent. The derivation builds `--bin vivarium-guest-agent --no-default-features --features guest` from the same tree the binary embeds.

## Consequences

- Good: no nested package exists, so the packaged crate carries every file `build.rs` embeds.
- Good: the agent's dependencies stay out of a host install, and the host's stay out of a guest image.
- Bad: the agent loses its separate build unit. Its tests compile only under the `guest` feature, so every lane passes `--all-features`.
- Bad: the package gains a feature whose only consumer is the guest derivation. A separate agent package returns only if a general use for the agent appears.

## Status

Implemented

Amends [`ADR-0102-the-installation-supplies-vivarium.md`](./ADR-0102-the-installation-supplies-vivarium.md): the embedded tree carries the agent as a binary of the crate rather than as a nested package, and the embedding itself stands. Answers [`Q-039`](../plan/open-questions.md).

Enacted in [`../../Cargo.toml`](../../Cargo.toml), whose `vivarium-guest-agent` binary sits behind the `guest` feature, with its source under [`../../src/bin/vivarium-guest-agent/`](../../src/bin/vivarium-guest-agent/main.rs) and its derivation in [`../../nix/default.nix`](../../nix/default.nix). `cargo package --locked` verifies, and [`KI-0004`](../reference/known-issues/resolved/KI-0004.md) records the resolved case.
