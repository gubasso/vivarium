# Justfile — vivarium task runner.
# Recipes route through the pinned Nix devShell so tasks run in the same
# environment CI and direnv provide.

# List available recipes.
default:
    @just --list

# --- Build & test ---------------------------------------------------------
#
# The test recipes take a lane name and run that lane's pre-commit hook. Neither
# the command nor its flags appear here: `.pre-commit-config.yaml` is the one
# place a lane's command is written, and `just`, `git`, and CI all reach it by
# hook id. The predecessor of this block copied each hook's command as a "twin"
# with a comment asking a reader to keep the two byte-identical, which is a
# boundary held by prose — the kind this repository has watched get crossed.
#
# The recipes that compile go through `tests/host/heavy-run` (ADR-0106): it
# resolves `VIVARIUM_HEAVY_DRIVE` and binds the compile cache, scratch, and
# vivarium's own roots onto it before the command starts. Without a usable drive
# they refuse; `VIVARIUM_HEAVY_ON_HOST=1 just <recipe>` runs here anyway, which
# is what CI sets. The gate is outside `nix develop` rather than inside it,
# because `nix develop` realises the development environment into the store and
# a check that runs after that has already let the write it was meant to gate
# happen. The lane hooks gate themselves again from within; paying twice is
# cheaper than choosing which invocation may skip it.

# Build the workspace.
build:
    tests/host/heavy-run --need 4 --label "just build" -- nix develop --command cargo build --workspace

# Run one test lane: unit, local, eval, net, boot, or doc.
# `docs/reference/testing-lanes.md` says what each one needs and proves.
test LANE="local":
    tests/host/heavy-run --need 2 --label "just test {{LANE}}" -- nix develop --command pre-commit run --all-files --hook-stage manual test-{{LANE}}

# Run one host lane by name, e.g. `just lane base-image`.
# These boot real guests and several take tens of minutes; each gates its own disk.
lane NAME:
    nix develop --command pre-commit run --hook-stage manual lane-{{NAME}}

# Exactly what `git push` runs. Not a copy of it: the same stage, the same hooks.
push-checks:
    nix develop --command pre-commit run --all-files --hook-stage pre-push

# Type-check without producing binaries.
typecheck:
    tests/host/heavy-run --need 4 --label "just typecheck" -- nix develop --command cargo check --workspace

# --- Lint & format --------------------------------------------------------

# Format the source tree.
fmt:
    nix develop --command cargo fmt --all

# Run the commit-stage hook set over the tree — what CI's `hooks` job executes,
# so a contributor without hooks installed cannot pass CI without them. The push
# stage is `just push-checks`, and each lane is `just test <lane>`.
hooks:
    nix develop --command pre-commit run --all-files --hook-stage pre-commit

# --- Slides ---------------------------------------------------------------
# The Slidev deck under slides/. Nix supplies node through the devShell; npm
# supplies Slidev, pinned by slides/package-lock.json (ADR-0104).

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
