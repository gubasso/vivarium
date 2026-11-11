# KI-0004 investigation

## Symptom

`cargo package` fails at its verification step. `build.rs` reports `NotFound` for `crates/vivarium-guest-agent`, because the packaged tree has no `crates/` directory. Measured 2026-09-09, and recorded in [Q-039](../../../plan/open-questions.md).

## Root cause

Cargo leaves out of a package every directory that carries its own `Cargo.toml`. `crates/vivarium-guest-agent` is a workspace member, so it is such a directory, and an explicit `include` that names it changes nothing. `build.rs` embeds that source, because the guest-agent derivation builds from it, so the verification build of the packaged crate cannot find it.

## The mask

`release-plz.toml` sets `publish = false`. release-plz then tags a release and publishes nothing to crates.io. The cargo-dist installers and the Nix flake carry every release.

`release-plz.toml` also sets `git_only = true`. Without it, release-plz reads the current version from crates.io, which holds only 0.1.0. After v0.2.0 it then took 0.2.0 as unreleased and proposed no new release. Git-only mode reads the version from the `v*` tags.

## Recheck condition

Recheck when a change moves the guest-agent source into the main package, or makes the packaged tree carry it in another way. Run `cargo package --locked`. The case resolves when the verification build passes. Then remove `git_only` and set `publish = true` in `release-plz.toml`, run the release path of [`../../../guides/publishing.md`](../../../guides/publishing.md), and collapse this dossier under `resolved/`.
