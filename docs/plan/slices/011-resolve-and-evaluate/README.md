# 011 — Resolve and evaluate

## Goal

Turn a bound manifest into a guest system derivation, so that everything downstream has something real to boot.

## Appetite

3 implementation sessions.

## Core

One shipped example manifest, bound to a project, evaluates to a guest system derivation. The binding persists across invocations and the generated flake is inspectable on disk.

## In scope

Ordered, because each item is the input to the next and a later item cannot be judged before the one above it exists. Do not reorder without recording why.

1. Resolve the config root and the sandbox name, the seam every other command reaches the manifest through.
2. Parse the TOML manifest and reject an unknown key with the compatibility message the failure contract fixes.
3. Emit the generated flake and stage its lock in the data root.
4. Persist the binding in the state root and read it back.
5. Evaluate and build the guest system derivation from that flake.
6. Land the `Cli` and `ConfigEval` trials this slice's `Acceptance` names, unskipped.

## Out of scope

- Booting anything. Slice 012 owns the first boot.
- The JSON Schema contract, which is [slice 006](../006-generate-config-contract/README.md)'s.
- Generations, `viv update`, and store reclamation.
- Composition extensions beyond what one example manifest exercises.
- Ordered remainder, cut first when the appetite binds: `viv manifest list`, the `viv config sources` provenance rendering, and `--json` on every verb except the ones the trials read.

## Governed by

- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — defines the verbs this slice implements.
- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — defines the config, data, and state roots and the registry record.
- [`../../../reference/spec/03-artifact-model.md`](../../../reference/spec/03-artifact-model.md) — defines images, pieces, and manifests.
- [`../../../reference/spec/04-composition-and-determinism.md`](../../../reference/spec/04-composition-and-determinism.md) — defines the merge and determinism contract.
- [`../../../reference/spec/15-project-identity.md`](../../../reference/spec/15-project-identity.md) — defines how a project resolves to a name.
- [`../../../explanation/configuration-and-composition.md`](../../../explanation/configuration-and-composition.md) — owns the resolution and composition topology.
- [`../../../explanation/state-and-lifecycle.md`](../../../explanation/state-and-lifecycle.md) — owns the state-root layout this slice writes to.
- [`../../../decisions/ADR-0002-module-system-as-composition-engine.md`](../../../decisions/ADR-0002-module-system-as-composition-engine.md) — fixes the merge engine as the module system, not a bespoke one.
- [`../../../decisions/ADR-0003-images-pieces-manifests-model.md`](../../../decisions/ADR-0003-images-pieces-manifests-model.md) — fixes the artifact vocabulary.
- [`../../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md) — fixes the compilation direction.
- [`../../../decisions/ADR-0005-xdg-user-config-layout.md`](../../../decisions/ADR-0005-xdg-user-config-layout.md) — fixes where the config root lives.
- [`../../../decisions/ADR-0006-manifest-binding-and-precedence.md`](../../../decisions/ADR-0006-manifest-binding-and-precedence.md) — fixes binding and precedence.
- [`../../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../../decisions/ADR-0011-config-read-only-binding-in-state.md) — fixes the binding as read-only state.
- [`../../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../../decisions/ADR-0015-cli-output-and-failure-contract.md) — fixes output streams and the failure contract.
- [`../../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../../decisions/ADR-0026-global-flags-and-config-precedence.md) — fixes global flags and their precedence.
- [`../../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md`](../../../decisions/ADR-0028-exit-code-taxonomy-and-stability.md) — fixes the exit-code taxonomy.
- [`../../../decisions/ADR-0029-project-identity-and-marker.md`](../../../decisions/ADR-0029-project-identity-and-marker.md) — fixes project identity.
- [`../../../decisions/ADR-0033-error-handling-and-exit-codes.md`](../../../decisions/ADR-0033-error-handling-and-exit-codes.md) — fixes how a typed error becomes a process code.
- [`../../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md`](../../../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md) — fixes the resolution seam item 1 builds.
- [`../../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md`](../../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md) — fixes the state-root file layout.
- [`../../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md`](../../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md) — fixes atomicity and lock ordering.
- [`../../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — fixes the generated flake's status.
- [`../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — fixes lock ownership and location.
- [`../../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md`](../../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md) — fixes the shipped example the core evaluates.

## Acceptance

When a project is bound to a shipped example manifest, `viv init` SHALL persist the binding and `workflow_01_first_time_bind_usage` SHALL pass unskipped.

Where no project-local config exists, resolution SHALL fall back to the global registry and `workflow_02_clean_repo_global_registry_only` SHALL pass unskipped.

When a shared manifest and a personal override are both present, evaluation SHALL apply the fixed precedence and `workflow_03_team_shared_and_personal_override` SHALL pass unskipped.

When the bound manifest is evaluated, `viv config eval` SHALL produce a guest system derivation path and `workflow_04_inspect_before_run` SHALL pass unskipped.

While a command is invoked with usage errors only, `workflow_04_inspect_before_run_usage`, `workflow_06_exec_usage_surface`, `workflow_07_volume_list_requires_binding`, and `workflow_08_destroy_usage_surface` SHALL pass unskipped at the `Cli` gate.

If the manifest carries an unknown key, then the failure SHALL carry all five parts the compatibility message fixes.

## Rabbit holes

- Building the whole command surface because the usage trials touch it — escape: implement the usage and failure paths those four `Cli` trials assert and nothing behind them; the verbs themselves belong to slices 012 through 014.
- Designing the composition surface past what one example needs — escape: the core is one shipped manifest; a second example is remainder.
- Re-deciding the generated flake's shape — escape: it is a regenerable cache artifact, so its internals are outside the stability promise and outside review.
- Making the state root general before it holds anything — escape: persist the binding the trials read back, and let slice 014 add what persistence it needs.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, the shipped example manifest evaluates to a guest system derivation on a capable host, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved to Implemented, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
