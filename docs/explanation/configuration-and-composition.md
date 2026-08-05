# Configuration and composition

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

One project resolves to one manifest through a fixed precedence chain, the one [ADR-0011](../decisions/ADR-0011-config-read-only-binding-in-state.md) settled when it superseded [ADR-0006](../decisions/ADR-0006-manifest-binding-and-precedence.md). The manifest is personal configuration: it selects an image, an ordered set of pieces, and launch declarations. Images establish reusable guest bases; pieces add small concerns; a manifest unifies them without introducing another merge language.

The CLI resolves names from the user's configuration libraries, validates the closed manifest grammar, and generates a flake that imports the chosen modules. The NixOS module system performs the merge. Build-channel values become module inputs; launch-channel values are evaluated and carried to the runner without making host-specific values build inputs. Content defects fail at the evaluation boundary rather than surfacing during launch.

Generated flakes are regenerable cache artifacts. Exactly one effective lock is in force: a team's read-only override lock beside the manifest when one is present, and otherwise the per-target lock vivarium owns under the data root. The override lock lives inside the config root and the tool never writes it, which is why `viv update` refuses rather than rewriting a team's pins; [config and XDG layout](../reference/spec/02-config-and-xdg-layout.md) owns the placement. Shared images and pieces declare their own flake inputs, and resolution refuses a declared input missing from the effective lock. A manifest may extend one local module only in its directory form; this is an escape hatch into the same module system, not manifest inheritance.

Exact layouts, key tables, defaults, precedence, and failure codes live in [config and XDG layout](../reference/spec/02-config-and-xdg-layout.md), [artifact model](../reference/spec/03-artifact-model.md), and [composition and determinism](../reference/spec/04-composition-and-determinism.md).

## Governing decisions

- [ADR-0002](../decisions/ADR-0002-module-system-as-composition-engine.md) — makes the NixOS module system the merge engine, so no second merge language exists.
- [ADR-0003](../decisions/ADR-0003-images-pieces-manifests-model.md) — fixes images, pieces, and manifests as the composition model.
- [ADR-0004](../decisions/ADR-0004-toml-manifest-compiles-to-flake.md) — fixes the TOML manifest that compiles to a generated flake.
- [ADR-0005](../decisions/ADR-0005-xdg-user-config-layout.md) — fixes the config root this chain resolves against.
- [ADR-0008](../decisions/ADR-0008-two-layer-separation.md) — separates the sandbox layer from the project's own environment.
- [ADR-0011](../decisions/ADR-0011-config-read-only-binding-in-state.md) — makes the config root read-only to the tool and moves the binding into state, superseding [ADR-0006](../decisions/ADR-0006-manifest-binding-and-precedence.md).
- [ADR-0012](../decisions/ADR-0012-generate-config-examples-from-types.md) — derives schema and examples from the config types instead of scaffolding files.
- [ADR-0020](../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes one declarative schema for extra mounts and host-config mirroring.
- [ADR-0021](../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md) — keeps launch-channel options typed inside the module system.
- [ADR-0040](../decisions/ADR-0040-manifest-is-the-personal-layer.md) — makes the manifest the personal layer rather than a place for shared guarantees.
- [ADR-0041](../decisions/ADR-0041-resource-and-volume-channel-classification.md) — classifies resource and volume options onto the build or launch channel.
- [ADR-0042](../decisions/ADR-0042-evaluation-time-content-defects.md) — makes a merged-configuration defect fail at the evaluation boundary.
- [ADR-0045](../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md) — fixes library layout and how a name resolves to an artifact.
- [ADR-0046](../decisions/ADR-0046-global-config-file-and-precedence.md) — fixes the global config file and its position in precedence.
- [ADR-0047](../decisions/ADR-0047-manifest-carries-no-schema-version.md) — refuses a schema-version key, leaving the key set as the compatibility signal.
- [ADR-0057](../decisions/ADR-0057-manifest-grammar-and-validation-boundary.md) — closes the manifest key table and fixes one validation boundary.
- [ADR-0058](../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — makes the generated flake a regenerable cache artifact.
- [ADR-0059](../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — puts the tool-owned lock per target in the data root.
- [ADR-0060](../decisions/ADR-0060-extends-is-one-local-module.md) — limits `extends` to one local module and rejects manifest inheritance.
- [ADR-0061](../decisions/ADR-0061-examples-ship-not-a-second-namespace.md) — ships examples rather than a second resolution namespace.
- [ADR-0062](../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md) — scopes the override lock to one manifest and makes `viv update` refuse under it.
- [ADR-0063](../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md) — requires the directory manifest form so the copied unit stays bounded.
- [ADR-0073](../decisions/ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md) — lets an image or piece declare the inputs it needs.
- [ADR-0074](../decisions/ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md) — pins declared inputs through the one effective lock and refuses a missing node.

## Unresolved

- Type-driven generation of schema, examples, and defaults belongs to [slice 006](../plan/slices/006-generate-config-contract/README.md).
