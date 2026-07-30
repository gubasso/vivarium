# 04 — Composition and determinism

How the layers named by a manifest merge into one configuration, and what makes the resulting VM reproducible. The decision to reuse the module system rather than build a merge engine is in [`../../decisions/ADR-0002-module-system-as-composition-engine.md`](../../decisions/ADR-0002-module-system-as-composition-engine.md).

## Composition is module merge

A manifest resolves to an image plus an ordered list of pieces (see [`03-artifact-model.md`](./03-artifact-model.md)). The tool imports them as NixOS modules and the module system merges them. There is no separate vivarium merge engine.

The merge is **priority-based, not order-based**:

- **Lists concatenate.** Every layer that contributes to a list — package sets, mount lists, the egress allowlist — adds to it; the effective value is the union of all layers.
- **Scalars resolve by priority.** A scalar set with `mkDefault` yields to a normally-set scalar, which yields to one set with `mkForce`. Position in the `pieces` list does not decide the winner; priority does — and it never breaks a tie either. Two definitions surviving at the same priority are a content defect that fails evaluation with `65`, not an order-resolved win (see [`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)).

## The priority convention

vivarium assigns roles to priorities so composition is predictable. Three priorities carry four roles:

- **Base defaults** — images set overridable values with `mkDefault`.
- **Piece proposals** — a shared piece that merely suggests a value uses `mkDefault` too, so the user's manifest can still decide. This is what lets two independently-authored pieces be adopted together: at normal priority they would collide instead.
- **Personal leaf** — the manifest's own settings use normal priority and so override every default. The manifest is the **personal** layer ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)), so this tier is where one user's own choices win.
- **Hard floor** — a shared piece that must not be overridden (for example a security policy) uses `mkForce`. This is how a team makes a guarantee unwaivable: it lives in a shared piece, and no personal manifest outranks it.

Two disagreeing `mkDefault` proposals are only a conflict while nothing stronger is set; the moment the manifest sets the value itself, its normal-priority definition outranks both and the conflict disappears. That is the reason proposals belong at `mkDefault`.

This gives the ergonomics of layered overrides — "your manifest overrides the defaults, the security floor overrides everything" — without any custom ordering logic. `viv config eval` (see [`01-command-surface.md`](./01-command-surface.md)) renders the merged result so users can see the effective configuration; `viv config sources` shows which layer each value came from.

## The build and launch channels

The merged configuration divides into two channels, decided in [`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md):

- **Build channel** — everything the guest system derivation depends on: packages, services, policy. This is what `nix build` realizes into the immutable image.
- **Launch channel** — runtime declarations under the tool-owned options `vivarium.mounts`, `vivarium.env`, and `vivarium.resources`; the manifest's `[[mounts]]`, `[env]`, and `[resources]` tables compile into the same options. The tool reads this channel by **pure evaluation** of the merged configuration and applies it when the VM launches; no build output may depend on it (N19, [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)).

Membership is decided per option by what depends on the value, not by the `vivarium.*` prefix ([`../../decisions/ADR-0041-resource-and-volume-channel-classification.md`](../../decisions/ADR-0041-resource-and-volume-channel-classification.md)). `vivarium.volumes` is the exception that proves it: a volume's guest mountpoint is guest system configuration, so it belongs to the **build** channel and adding a volume rebuilds. Only its host image path and virtual size resolve at launch.

The split is what lets a shared piece carry host-facing declarations without breaking purity: host-side variables in mount sources stay unexpanded through evaluation and resolve against the host environment only at launch (see [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). Typing lives in the options module: `vivarium.mounts`, `vivarium.env`, `vivarium.resources`, and `vivarium.volumes` are all declared with module option types, so a malformed declaration fails evaluation with a precise error rather than at boot.

## What gets built: the generated flake

The manifest is not evaluated directly. The tool compiles it into a **generated flake** and builds that ([`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md)); the flake's module `imports` are the resolved image, the ordered pieces, the `extends` module when one is named, and a manifest leaf carrying the TOML's own values. The artifact itself is decided in [`../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md):

- It lives under the **cache** root, at the path [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md) fixes, and is regenerated wholesale — safe to delete, never edited by hand, and printed by `viv config` for anyone debugging a merge.
- The resolved modules are **copied into it**, not referenced from the config root. Referencing would make every unrelated image, piece, and other project's manifest a build input, so editing any of them would change this project's output path — and the output path is the freshness key below, so every project would rebuild.
- `images/` and `pieces/` copy wholesale; the manifest library does not. When a manifest names `extends`, the one exception is that manifest's **own directory**, which is why naming `extends` requires the directory form — the copied unit must hold the module's helpers and no other manifest ([`03-artifact-model.md`](./03-artifact-model.md), [`../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md`](../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md)).
- Because the manifest's own text becomes a module in that tree, **the manifest lands in the store** like any other layer — including its launch-channel tables. N19 still holds: no build output depends on a launch-channel value. What reaches the store is the text, which is why nothing secret belongs in a manifest ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md), N10).

## Determinism

A sandbox is a Nix build, and its inputs are pinned by **exactly one effective lockfile**: a team's read-only override lock beside the manifest when one is present, and otherwise the per-target lockfile vivarium owns under the data root. Which of the two is in force is decided in [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), which owns the rule, and `viv config` reports the answer ([`01-command-surface.md`](./01-command-surface.md), [`../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)). "Exactly one" is literal: no second lock is maintained in the background, which is why `viv update` refuses rather than writing the file an override shadows ([`../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md`](../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md)). The tool-owned lock is created by the first build or the first `viv update`, whichever comes first, and reported; thereafter it **moves only when `viv update` moves it** — creating a pin is not re-resolving one, and no ordinary build re-resolves inputs, which is what makes the guarantee below hold over time rather than only within one afternoon. Given the same manifest closure and the same lockfile, the build evaluates to the same store output on any machine and at any later time. Two consequences follow:

- **The store output hash is the freshness key.** Identical inputs produce an identical output path; the tool does not compute a separate content digest. A changed layer changes the inputs, which changes the output path, which triggers a rebuild. Each such output the tool retains is pinned as a **generation** ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).
- **The build must be pure.** No host-specific value may enter it. In particular, the working directory path is injected at launch time, never built in, per [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md).

The determinism and purity requirements are stated normatively in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md).
