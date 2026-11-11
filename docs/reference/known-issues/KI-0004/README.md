# KI-0004 — the crate does not package, so vivarium is not on crates.io

- Status: `masked`
- Severity: medium — `cargo install vivarium` is unavailable, and the cargo-dist installers and the Nix flake are the only channels.
- External system: Cargo, which leaves a nested package out of its parent's tarball.
- Affected checks: `cargo package`, and the `package-check` step of `rk setup check`.
- Mask: active. [`release-plz.toml`](../../../../release-plz.toml) sets `publish = false` and `git_only = true`. Remove `git_only` and set `publish` back to `true` when the recheck condition holds.

`build.rs` embeds the tree that an installed binary writes into every generated `vivarium/` subtree, and that tree includes `crates/vivarium-guest-agent`. Cargo puts no nested package, a directory with its own `Cargo.toml`, inside its parent's tarball. The packaged tree therefore has no `crates/`, and `build.rs` exits `NotFound` while `cargo package` verifies. A crate published past that check would fail to build for every user.

[Q-039](../../../plan/open-questions.md) holds the three shapes. The owner chose the first one, not publishing, until the third one, which keeps the embedding and makes the package verify, is built.

- [`issue.yaml`](./issue.yaml) — supplies the machine-readable case metadata.
- [`investigation.md`](./investigation.md) — owns the cause, the mask, and the recheck condition.
