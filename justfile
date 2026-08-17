# Justfile — vivarium task runner.
# Recipes route through the pinned Nix devShell so tasks run in the same
# environment CI and direnv provide.

# List available recipes.
default:
    @just --list

# --- Build & test ---------------------------------------------------------

# Build the workspace.
build:
    nix develop --command cargo build

# Run the test suite.
test:
    nix develop --command cargo nextest run

# Run the pre-commit unit-test profile (twin of hook cargo-nextest-unit).
test-pre-commit:
    nix develop --command cargo nextest run --profile pre-commit --all-features

# Run the pre-push integration-test profile (twin of hook
# cargo-nextest-integration).
test-pre-push:
    nix develop --command cargo nextest run --profile pre-push --all-features

# Run the complete CI profile.
# Twin of the cargo-nextest hooks in .pre-commit-config.yaml: keep the
# feature flags byte-identical so the two cannot drift.
test-ci:
    nix develop --command cargo nextest run --profile ci --all-features

# Type-check without producing binaries.
typecheck:
    nix develop --command cargo check

# --- Lint & format --------------------------------------------------------

# Run clippy with warnings denied.
# Twin of the clippy-strict hook in .pre-commit-config.yaml: keep the
# command byte-identical so the two cannot drift when a feature lands.
lint:
    nix develop --command cargo clippy --all-targets --all-features -- -D warnings

# Format the source tree.
fmt:
    nix develop --command cargo fmt

# Check formatting without rewriting files.
fmt-check:
    nix develop --command cargo fmt --check

# Format, lint, then test.
check: fmt lint test

# Run the whole hook set over the tree, at both gating stages — what CI's
# `hooks` job executes, so a contributor without hooks installed cannot pass
# CI without them. Skips at the push stage: the nextest hooks because
# `just test-ci` already runs profile `ci`, a superset of `pre-push`, and
# clippy-strict because `just lint` is its byte-identical twin in CI's lint
# job. `cargo-doc-tests` is NOT skipped — nextest cannot run doctests, so
# this is CI's only doctest coverage.
hooks:
    nix develop --command pre-commit run --all-files --hook-stage pre-commit
    SKIP=cargo-nextest-unit,cargo-nextest-integration,clippy-strict nix develop --command pre-commit run --all-files --hook-stage pre-push

# --- Slides ---------------------------------------------------------------
# The Slidev deck under slides/. Nix supplies node through the devShell; npm
# supplies Slidev, pinned by slides/package-lock.json (ADR-0103).

# Install the deck's pinned dependencies.
slides-install:
    nix develop --command bash -c 'cd slides && npm ci'

# Serve the deck with hot reload at http://localhost:3030.
slides-dev:
    nix develop --command bash -c 'cd slides && npm run dev'

# Build the deck into slides/dist under the deployed base path.
# Twin of the build step in .github/workflows/pages.yml: that workflow derives
# the base from the repository name, so keep this literal equal to it.
slides-build:
    nix develop --command bash -c 'cd slides && npm run build -- --base /vivarium/'

# --- Developer install ----------------------------------------------------
# Install logic lives in scripts/install-dev, never in this file. The devShell
# already puts a `viv` built from the working tree on PATH inside this
# repository; these recipes are for using it anywhere else.

# Install `viv` and `vivarium-supervisor` into cargo's bin directory.
install:
    nix develop --command scripts/install-dev

# Same, plus a `viv-dev` that resolves its baseline inputs against this tree.
install-pinned:
    nix develop --command scripts/install-dev --pinned

# Remove `viv`, `vivarium-supervisor` and `viv-dev` again.
uninstall:
    nix develop --command scripts/install-dev --uninstall

# --- Publishing -----------------------------------------------------------
# These wrap the auth-gated helper scripts under scripts/; publish logic and
# the crates.io auth gate live there, never in this file.

# Dry-run crates.io readiness (no token required).
publish-dry *ARGS:
    nix develop --command scripts/publish-dry {{ARGS}}

# Publish to crates.io (auth-gated; the script checks auth is configured).
publish *ARGS:
    nix develop --command scripts/publish {{ARGS}}

# Local release helper (release-plz / cargo-release / semver-checks; see docs/guides/publishing.md).
release *ARGS:
    nix develop --command scripts/release {{ARGS}}

# Check public API compatibility (cargo semver-checks via the release helper).
semver-check *ARGS:
    nix develop --command scripts/release semver-check {{ARGS}}

# Open/refresh the release PR (release-plz release-pr via the release helper).
release-pr *ARGS:
    nix develop --command scripts/release release-plz-pr {{ARGS}}

# Update versions + changelog locally (release-plz update via the release helper).
release-update *ARGS:
    nix develop --command scripts/release release-plz-update {{ARGS}}
