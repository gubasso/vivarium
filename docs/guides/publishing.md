# Publish a vivarium release

Use this guide to publish vivarium to [crates.io](https://crates.io) and attach binary artifacts to the corresponding GitHub release. Start on `develop` with the intended changes merged, required checks green, and no release tag created by hand. Helper scripts live under `scripts/` (`publish-dry`, `publish`, `release`). Their authentication checks confirm only that crates.io credentials are configured; they do not validate a token.

## Choose the release path

- CI-first, recommended: `release-plz` runs on `develop` (the trunk) and opens a release PR that bumps the version and updates the changelog, `Cargo.toml`, and `Cargo.lock`. Merging that PR publishes the new version automatically and tags it; the tag-triggered `promote-to-master.yml` workflow then fast-forwards `master` onto that tag, so `master` holds only released commits.
- Local escape hatch: run the helper scripts by hand when CI is unavailable.

The branch model: `develop` is the trunk and the GitHub default branch (release-plz bases the release PR on the default branch); `master` is never written by hand — CI fast-forwards it onto each release tag. `master`'s ruleset bypass actor is the GitHub App, and `promote-to-master.yml` pushes `master` under that App token so the push is attributed to the bypass actor and accepted — on a personal account the default `GITHUB_TOKEN` identity (`github-actions[bot]`) cannot be a bypass actor, so a default-token push would be rejected. release-plz itself also runs under that App token so its tag push retriggers the tag-triggered workflows — both `promote-to-master.yml` and the cargo-dist binary builds (see [Binary distribution](#binary-distribution-cargo-dist)).

First-time crates.io setup — creating a scoped `publish-new` token, `cargo login`, the first manual `cargo publish`, then configuring Trusted Publishing and revoking the token — is a one-time manual requirement, not part of routine maintenance. See the crates.io Trusted Publishing docs: <https://crates.io/docs/trusted-publishing>.

## SemVer policy

For a library crate, `cargo-semver-checks` gates public-API compatibility and runs natively inside release-plz. Run it locally with `./scripts/release semver-check`. A binary-only crate has no public API to check but still follows semantic versioning for its releases.

## Routine automated release

You never hand-create the tag; the only manual actions are two merges.

1. Merge feature work (Conventional Commits) to `develop`.
2. release-plz opens/updates the release PR on `develop` (version bump + changelog).
3. Review the PR; merge it — the one human release decision.
4. release-plz tags the release (`vX.Y.Z`) and publishes to crates.io over OIDC.
5. The tag push (made with release-plz's GitHub App token) triggers two tag workflows: `promote-to-
   master.yml` fast-forwards `master` onto that tag, and the cargo-dist `release.yml` builds and attaches binaries — see [Binary distribution](#binary-distribution-cargo-dist).

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

vivarium is a CLI, so `dist` (cargo-dist) builds prebuilt binaries and attaches shell/PowerShell/Homebrew-tap installers to each GitHub Release; `cargo-binstall` then works automatically from those releases. It is separate from crates.io publishing and configured in `dist-workspace.toml`. `dist` generates its own workflow at `.github/workflows/release.yml`, distinct from the release-plz workflow (`release-plz.yml`), so the two never collide and the crates.io Trusted Publisher keeps matching the actual release-plz filename. Treat the generated YAML as an artifact: run `dist init` (first time) or `dist generate` after editing `dist-workspace.toml`, never hand-edit it. AUR, OBS/zypper, and Homebrew (beyond the generated tap) are downstream/manual channels that consume the tagged GitHub Release artifacts, not auto-generated pipelines.

The generated `.github/workflows/release.yml` fires on a pushed version tag (any tag carrying a semver, e.g. the `v0.1.0` tags release-plz creates), builds the configured targets, and attaches the installers to the GitHub Release. After editing `dist-workspace.toml`, regenerate it with `dist generate` and verify it is in sync with `dist generate --check`; never hand-edit the workflow.

Automatic trigger: release-plz runs with a GitHub App token (secrets `RELEASE_PLZ_APP_ID` / `RELEASE_PLZ_APP_PRIVATE_KEY`), so the tag it pushes retriggers `release.yml`. A tag pushed with the default `GITHUB_TOKEN` would not retrigger it; that is why the App token is required. Create a GitHub App with `contents` + `pull-requests` write, install it on the repo, and store its App ID and private key as those two secrets.

## Backend security owner

The hypervisor, filesystem daemon, and guest kernel are pinned by a project's lockfile rather than installed from a host's package manager, so an upstream fix reaches users only as a released pin move. One maintainer owns that watch. The [security policy](../../SECURITY.md) owns the clocks, response windows, and advisory contents. [ADR-0078](../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) and [ADR-0079](../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md) own the rationale. This table owns only the current assignment.

| Role                   | Holder                           |
| ---------------------- | -------------------------------- |
| Backend security owner | @gubasso                         |
| Backup                 | none — single-maintainer project |

The owner cell MUST name a person before the first release because `SECURITY.md` states a response target. `none` in the backup cell is a disclosed staffing fact. If vivarium gains a second maintainer, name the backup here; a rotation edits this table, never an ADR.

The reporting route in `SECURITY.md`, the repository's Report a vulnerability form, is a one-time repository setting under Settings → Advanced Security → Private vulnerability reporting. It was enabled on 2026-07-31 and MUST remain enabled.

## Manual release if CI is down

1. `./scripts/publish-dry` to validate.
2. `./scripts/release semver-check` (library crates).
3. Ensure auth is configured (`cargo login`).
4. `./scripts/publish`.

## Stop, yank, and recover

Before publication, stop instead of attempting to repair a questionable artifact in place. After publication, a version cannot be overwritten or deleted; it can only be yanked:

- `cargo yank --version X.Y.Z` — prevent new dependents from selecting it.
- `cargo yank --version X.Y.Z --undo` — reverse a yank.

Confirm the affected version before running a yank. If the wrong version is yanked, immediately run the `--undo` form. Fix the release by publishing a new version; under `0.x`, Cargo treats the minor component as the breaking position, so version accordingly.
