# ADR-0101: The check surface has one stage per claim

## Context and Problem Statement

The hook configuration installs three git stages, and a hook without an explicit `stages:` key runs at every one of them — so roughly thirty commit-stage hooks silently re-ran on `git push`, including autofixers, whose rewrites during a push cannot enter the push they just failed. Separately, CI ran only format, clippy, tests, and build, so a contributor without hooks installed bypassed every other gate. Boundaries held only by prose get re-crossed one plausible addition at a time — `scripts/check-flake-boundary` exists because exactly that happened to the root flake — and a hook added without a `stages:` key is that shape of addition.

## Considered Options

- One stage per claim, defaulted and enforced: `default_stages: [pre-commit]`, explicit `stages:` only where the push stage earns it, and a CI job running the same hook set
- Keep implicit all-stages behavior and prune hooks case by case
- Document the intended stages in comments only

## Decision Outcome

Chosen option: `one stage per claim, defaulted and enforced` — the stage a hook runs at follows from what its claim is about, not from configuration defaults.

The commit stage owns per-file checks and every autofixer. The push stage owns claims about the whole tree or the history: the integration and doc-test lanes, the strict clippy gate, full-history secret scanning, dependency policy, and product-flake evaluation. A hook that rewrites files never appears at push. CI's `hooks` job runs both stages over the tree via one `just hooks` recipe, so the local gates and CI cannot diverge and hookless contributors cannot bypass them; upstream hooks that pin their own stages are overridden per hook.

## Consequences

- Good: each hook runs once at the stage whose claim it serves; a push pays for push-scope claims only; CI enforces the same surface locally installed hooks do.
- Bad: a new hook must decide its stage explicitly, and a wrong choice (a push-only fixer) is caught by review rather than by machinery.

## Status

Implemented

Enacted in `.pre-commit-config.yaml` (`default_stages` and per-hook `stages:`), `justfile` (`hooks`), and `.github/workflows/ci.yml` (`hooks` job, plus a `committed` lane over each event's commit range for the commit-msg stage `pre-commit run --all-files` cannot replay).

Amended by [`./ADR-0114-a-lane-runs-where-a-place-names-it.md`](./ADR-0114-a-lane-runs-where-a-place-names-it.md) — one stage per claim stands, and what changed is how a place names its claims. `just` and CI now invoke hooks by id instead of carrying twin commands, and CI runs the push-stage claims as a matrix of named hooks rather than a second whole-stage sweep.
