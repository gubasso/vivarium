# vivarium — Charter

## What this is for

vivarium makes a project's declarative environment runnable inside a reproducible per-project microVM boundary while leaving project-authored files and the inner development environment untouched.

## Pillars

- Separate-kernel isolation.
- Pure, deterministic composition through existing Nix mechanisms.
- User-owned project files and secrets remain outside builds.
- Elastic multi-sandbox economics without hidden arbitration.
- Evidence-backed changes with exact current-state reporting.

## No-gos

- No shared-kernel mode.
- No bespoke merge engine.
- No secrets in the Nix store.
- No product outputs in the root development flake.
- No guest durability treated as a system of record.
- No background resource arbitration.
- No claims about target hosts derived from agent environments.

The [normative specification](../reference/spec/README.md) owns the product contract behind these boundaries.

## Appetite unit

The project uses implementation sessions as its permanent appetite unit. One implementation session is one fresh execution context ending in a reviewable checkpoint. An appetite is a fixed budget chosen before shaping, not a forecast; when it binds, cut the ordered remainder or reshape instead of silently extending it.
