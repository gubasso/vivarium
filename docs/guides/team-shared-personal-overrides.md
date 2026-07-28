# Combine team configuration with a personal override

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow to share a manifest and reusable pieces while keeping machine-specific choices personal. The old `.vivarium.toml` plus `.vivarium.local.toml` mental model is obsolete: follow the [XDG config-library and registry model](../reference/spec/02-config-and-xdg-layout.md) instead.

## Author the layers

Place the shared manifest and pieces in the config library, add the personal piece there, and use portable mount variables as described by [secrets and configuration sharing](../reference/spec/07-secrets-and-config-sharing.md). Choose priorities using the roles in [composition and determinism](../reference/spec/04-composition-and-determinism.md).

## Bind and inspect the merge

```console
$ viv init --manifest shared-personal --write --yes
$ viv config eval --json
$ viv config sources --json
```

Use the evaluated view for the effective result and the sources view for attribution or ties. The exact envelopes belong to the [command surface](../reference/spec/01-command-surface.md).

## Acceptance coverage

The gated trial [`workflow_03_team_shared_and_personal_override`](../../tests/user_workflows.rs) checks the real config-root model, portable mount value, winning layer, tie reporting, and fail-closed shared-path guard.
