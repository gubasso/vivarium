# ADR-0112: The selected image carries the base flake

## Context and Problem Statement

[`spec/03`](../reference/spec/03-artifact-model.md) reserves `nixpkgs` and `microvm`, refusing any artifact that declares one (`78`). The reservation is right about collision between shared artifacts and silent about ownership: no user can say what their own guest — kernel, packages, hypervisor ([`ADR-0049`](./ADR-0049-backend-is-a-closure-member.md)) — resolves from. [`ADR-0102`](./ADR-0102-the-installation-supplies-vivarium.md) fixed that vivarium supplies the tool, never the build; this extends the line to the image.

## Considered Options

- Unreserve the baseline names for every artifact's `inputs.toml`
- A redirect table in the manifest
- The selected directory-form image carries a real `flake.nix` beside its `default.nix`

## Decision Outcome

Chosen option: the image carries a real flake — redirecting your own project is a different act from a shared artifact colliding, so the reservation stands and ownership moves.

The generated flake takes the base as an input named after the image, `path:./images/<name>` into the copy the tree already materializes. Declared baselines — explicit, or `outputs` arguments — are followed (`nixpkgs.follows = "<name>/nixpkgs"`); undeclared ones keep vivarium's reference. The base contributes inputs only; vivarium keeps the outputs skeleton; a named module attribute may open later. After locking, the lock in force must resolve `nixpkgs` and `microvm` to one tree each or the build refuses (`78`).

Measured on Nix 2.34.8 (findings register, 2026-08-26): a relative path node locks as `path` plus `type` — no hash — so the base's text is a layer whose edits flow into the next build, while its declared references are pinned nodes only `viv update` moves. The absolute form was rejected for silently serving stale content against its lock. A base's own `flake.lock` seeds every re-lock of its subtree: the user's own pin statement, symmetric with the team override lock.

## Consequences

- Good: every guest component's reference is user-ownable, no vivarium release in the path.
- Good: one tool-maintained lock still pins the build; `ADR-0058`'s objection targeted hashed path nodes, which a relative node is not.
- Bad: base edits change builds without `viv update` — deliberate layer semantics, as with a manifest edit.
- Bad: a base sub-lock governs seeding silently; `viv update` must say why nothing moved.

## Status

Implemented

Enacted by [slice 018](../plan/slices/018-the-user-owns-the-image/README.md): the probe and collision refusals are [`../../src/config/base.rs`](../../src/config/base.rs), the four-case rendering [`../../src/config/flake.rs`](../../src/config/flake.rs), the pins and the composed check [`../../src/config/lock.rs`](../../src/config/lock.rs), the private update tree and both persistence paths [`../../src/config/materialize.rs`](../../src/config/materialize.rs), and the verb [`../../src/cli/update.rs`](../../src/cli/update.rs). Proved end to end by [`../../tests/host/update-check`](../../tests/host/update-check), whose first complete run is in the harness [findings register](../reference/microvm-verification-harness.md).

Amends [`ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](./ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — the generated flake gains one `path:` input into its own materialized `images/` copy; self-containment and the freshness key survive because the relative node carries no hash to re-lock.

Amends [`ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](./ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — the creating resolution may be seeded by a base's own lock vivarium does not own; only `viv update` still moves the tool-owned lock.

Amends [`ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md`](./ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md) — the baseline reservation stands for `inputs.toml`; the selected image's own flake is the sanctioned redirect route, and the image name joins `viv update`'s namespace.

Amends [`ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md`](./ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md) — "exactly one effective lockfile" means one lock maintained by the tool; a base's own lock seeds resolution of its subtree. Updating a followed baseline updates the base input that owns its node.

Amends [`ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](./ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — a fix reachable from a reference the user owns needs no vivarium release, only the user's own `viv update`; the release remains the vehicle for the shipped default.
