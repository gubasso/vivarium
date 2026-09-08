# Publish a vivarium release

Use this guide to publish vivarium to [crates.io](https://crates.io) and attach binary artifacts to the corresponding GitHub release. This repository runs the release-kit convention, so the convention owns the procedure and this page owns what is specific to vivarium. Read `rk method operate` for the release itself and `rk guide release` for its commands. Helper scripts live under `scripts/` (`publish-dry`, `publish`, `release`). Their authentication checks confirm only that crates.io credentials are configured; they do not validate a token.

## The branch model

`master` is the one long-lived branch. It is the trunk, the GitHub default branch, and the only branch a release is cut from. It takes no direct push: every change reaches it as a squash-merged pull request from a short-lived branch that lives in its own linked worktree. `rk worktree add <type>/<slug>` creates that worktree, and `rk method worktrees` owns the reasoning.

The release style is `trunk`, which means continuous release. release-plz keeps one release request open against `master`, and that request is armed from the moment it opens: it merges itself as soon as every required check passes, and the merge publishes. There is no separate human release decision. To hold a release, disarm the request before its last check goes green; a release held too late is withdrawn rather than abandoned.

Because the changelog is built from the squash titles and bodies that land on `master`, the quality of a release note is decided at merge time. The landed `pr-title` check and the `rk-message` hook are what hold that.

release-plz runs under a GitHub App token (secrets `RELEASE_PLZ_APP_ID` and `RELEASE_PLZ_APP_PRIVATE_KEY`), because a tag pushed with the default `GITHUB_TOKEN` starts no further workflow. The App token is what makes the tag push trigger the cargo-dist build in `release.yml`.

First-time crates.io setup — creating a scoped `publish-new` token, `cargo login`, the first manual `cargo publish`, then configuring Trusted Publishing and revoking the token — is a one-time manual requirement, not part of routine maintenance. See the crates.io Trusted Publishing docs: <https://crates.io/docs/trusted-publishing>.

## SemVer policy

`release-plz.toml` sets `semver_check = false`. vivarium ships two binaries and its lib target exists for this crate's own integration tests, so no external consumer holds the API. Run the check by hand with `./scripts/release semver-check` if that ever changes, and set the key to `true` in the same change. Releases still follow semantic versioning.

## Routine automated release

You never hand-create the tag, and under the `trunk` style you never merge the release request either.

1. Land feature work on `master` through a squash-merged pull request, with a scoped Conventional Commit title.
2. release-plz opens or updates the release request on `master`, bumping the version and the changelog.
3. Every required check passes, and the request merges itself.
4. release-plz tags the release (`vX.Y.Z`) and publishes to crates.io over OIDC.
5. The tag push, made with release-plz's GitHub App token, triggers the cargo-dist `release.yml`, which builds the binaries, attests them, and attaches them to the GitHub release — see [Binary distribution](#binary-distribution-cargo-dist).

## Local operator release

Use the local path only when CI is unavailable. Run the readiness checks first, then:

- `./scripts/release release-plz-update` — update versions + changelog locally.
- `./scripts/release release-plz-pr` — open/refresh the release PR.
- `./scripts/release cargo-release-dry <level>` — dry-run a `cargo-release` bump.
- `./scripts/release semver-check` — check API compatibility.

## Verify readiness

Run `./scripts/publish-dry`. It runs `cargo publish --dry-run` and `cargo package --list`; neither needs a token. Stop if the package fails to build, contains unintended files, omits required files, or the version/changelog is not ready. Resolve the discrepancy before publishing.

## Package contents (keep the tarball lean)

Cargo packages the whole working tree by default, so project docs, CI, and dev tooling ship as dead weight unless trimmed. Check `cargo package --list` and keep the `.crate` to build inputs plus `README`/`LICENSE`/`CHANGELOG`. Prefer an `exclude` denylist in `Cargo.toml` — it is robust against dropping future `src/` files:

```toml
[package]
exclude = [
    "/docs",
    "/.github",
    "/scripts",
    "/release-plz.toml",
    "/dist-workspace.toml",
    "/justfile",
    "/flake.nix",
    "/.pre-commit-config.yaml",
]
```

Footgun: with an SPDX `license` expression (e.g. `MIT`), Cargo does not auto-include a plain `README` or `LICENSE`, so an `include` allowlist must list them explicitly. crates.io enforces a hard 10 MB limit; for a binary crate no consumer reads the tarball at all, so docs and tooling are pure waste.

## Binary distribution (cargo-dist)

vivarium is a CLI, so `dist` (cargo-dist) builds prebuilt binaries and attaches shell and PowerShell installers to each GitHub Release; `cargo-binstall` then works from those releases with no configuration. It is separate from crates.io publishing and configured in `dist-workspace.toml`. The generated workflow is `.github/workflows/release.yml`, distinct from `release-plz.yml`, and only `release-plz.yml` is registered at crates.io as the trusted publisher. AUR, OBS/zypper, and Homebrew are downstream manual channels that consume the tagged GitHub Release artifacts.

`release.yml` fires on a pushed version tag, builds the configured targets, and attaches the installers to the GitHub Release. `dist-workspace.toml` sets `github-attestations = true` and `github-attestations-phase = "host"`, so every asset that reaches the release page carries a GitHub Artifact Attestation. crates.io stores no provenance of its own, which makes the release artifacts the only verifiable half of a vivarium release. A consumer checks one with:

```bash
gh attestation verify <file> --repo gubasso/vivarium
```

`release.yml` is generated and then hardened by hand, which is the one exception to the convention's rule against editing a generated workflow. `dist-workspace.toml` sets `allow-dirty = ["ci"]` to protect that hardening, and the file itself carries the reason: the generated form interpolates `${{ }}` expressions straight into four `run:` bodies, leaves two paths unquoted, and word-splits a flag, which the project's `zizmor` hook reports as 4 high and 3 medium findings. The cost is that `dist` no longer ports its own upgrades into the file. [`../reference/tracking.yaml`](../reference/tracking.yaml) carries that obligation with a cadence: on every `cargo-dist-version` bump, regenerate in a scratch copy, diff against the committed workflow, and port every change by hand except the hardening.

The `dist-plan` job in [`../../.github/workflows/ci.yml`](../../.github/workflows/ci.yml) proves that `dist` runs at the configured pin and that the configuration produces a viable release. It votes in the `gate` job, which is the one check the trunk's protection requires.

## Backend security owner

The hypervisor, filesystem daemon, and guest kernel are pinned by a project's lockfile rather than installed from a host's package manager, so an upstream fix reaches users only as a released pin move. One maintainer owns that watch. The [security policy](../../SECURITY.md) owns the clocks, response windows, and advisory contents. [ADR-0078](../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) and [ADR-0079](../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) own the rationale. This table owns only the current assignment.

| Role                   | Holder                           |
| ---------------------- | -------------------------------- |
| Backend security owner | @gubasso                         |
| Backup                 | none — single-maintainer project |

The owner cell MUST name a person before the first release because `SECURITY.md` states a response target. `none` in the backup cell is a disclosed staffing fact. If vivarium gains a second maintainer, name the backup here; a rotation edits this table, never an ADR.

The reporting route in `SECURITY.md`, the repository's Report a vulnerability form, is a one-time repository setting under Settings → Advanced Security → Private vulnerability reporting. It was enabled on 2026-07-31 and MUST remain enabled.

## Manual release if CI is down

`rk method recovery` owns this path and orders its steps. Trusted-publishing enforcement, once enabled at crates.io, rejects a token publish, so turn that switch off first. Then:

1. `./scripts/publish-dry` to validate.
2. Make sure that authentication is configured (`cargo login`).
3. `./scripts/publish`.
4. Turn enforcement back on.

## Stop, yank, and recover

Before publication, stop instead of attempting to repair a questionable artifact in place. After publication, a version cannot be overwritten or deleted; it can only be yanked:

- `cargo yank --version X.Y.Z` — prevent new dependents from selecting it.
- `cargo yank --version X.Y.Z --undo` — reverse a yank.

Confirm the affected version before running a yank. If the wrong version is yanked, immediately run the `--undo` form. Fix the release by publishing a new version; under `0.x`, Cargo treats the minor component as the breaking position, so version accordingly.
