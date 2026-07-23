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

# Type-check without producing binaries.
typecheck:
    nix develop --command cargo check

# --- Lint & format --------------------------------------------------------

# Run clippy with warnings denied.
lint:
    nix develop --command cargo clippy --all-targets -- -D warnings

# Format the source tree.
fmt:
    nix develop --command cargo fmt

# Check formatting without rewriting files.
fmt-check:
    nix develop --command cargo fmt --check

# Format, lint, then test.
check: fmt lint test

# --- Publishing -----------------------------------------------------------
# These wrap the auth-gated helper scripts under scripts/; publish logic and
# the crates.io auth gate live there, never in this file.

# Dry-run crates.io readiness (no token required).
publish-dry *ARGS:
    nix develop --command scripts/publish-dry {{ARGS}}

# Publish to crates.io (auth-gated; the script checks auth is configured).
publish *ARGS:
    nix develop --command scripts/publish {{ARGS}}

# Local release helper (release-plz / cargo-release / semver-checks; see PUBLISHING.md).
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
