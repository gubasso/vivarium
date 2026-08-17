# 015 — The binary supplies itself

## Goal

Remove the generated flake's `vivarium` input so the installed `viv` is the only vivarium a launch ever runs, and turn any remaining version skew into a named refusal before boot.

## Appetite

3 implementation sessions.

## Core

A generated project flake declares no `vivarium` flake input, no host-side launch step executes a Nix-built `viv`, and a selected build whose launch contract disagrees with the running binary is refused before boot with both versions named.

## In scope

Ordered; when the appetite binds, cut from the bottom.

1. Sweep the host safely before any live verification, and enumerate before removing anything. What an earlier vivarium installed or wrote is stale by construction under [`ADR-0102`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md), so the sweep covers retained per-target locks carrying a `vivarium` node, generated flake trees under the cache root, build records and gcroots naming superseded outputs, store paths of any Nix-built `viv` or supervisor, stranded backend units, and the hand-made lock backup from the incident this slice removes. Only what vivarium owns is touched. Anything sized in gigabytes goes through [`tests/host/disk-preflight`](../../../../tests/host/disk-preflight); a store collection is the operator's call, not the sweep's, because [`Q-023`](../../open-questions.md) prices it at a cold first boot. Record what the enumeration found, because the sweep is also this slice's first measurement of how the old shape actually behaves — [`../../../guides/develop-and-install.md`](../../../guides/develop-and-install.md) documents it failing loudly at evaluation on a NAR mismatch, while the incident behind this slice saw it succeed against stale content, and the two cannot both be the whole rule.
2. Make the binary carry the product Nix tree and write it into the generated tree on every preparation, referenced relatively — the same seam `vivarium-options.nix` and `vivarium-report.nix` already use — evaluated against the project's own `nixpkgs` and `microvm` so the current `follows` semantics hold. Delete the flawed shape in the same change, leaving no dead seam behind: the `vivarium.url` and `follows` rendering in [`src/config/flake.rs`](../../../../src/config/flake.rs), the `--bins` build of the tool inside its own guest derivation in [`nix/default.nix`](../../../../nix/default.nix), the `@vivPath@` exec ending [`nix/runner.sh`](../../../../nix/runner.sh), and the `VIVARIUM_BASELINE_VIVARIUM` seam with the third pin in [`scripts/baseline-pins`](../../../../scripts/baseline-pins). The supervisor comes from the running installation; the guest agent stays Nix-built, because it runs inside the guest and its control handshake already refuses a version it cannot speak.
3. Give every launch record and boot record a permissive version envelope read before the strict schema, so a record from another version yields its `schemaVersion` to every `viv` forever. A mismatch gets its own diagnostic naming both numbers and the remedy, distinct from corruption; `viv status` reports the skew beside `running` instead of a bare truth half the tool cannot act on.
4. Refuse a stale selected build before boot: the built output exposes its launch contract schema at a stable path, ensure-running reads it before executing the runner, and a mismatch exits `78` naming both versions and `--rebuild` or a newer generation as the exit. This is what still earns its place once the input is gone, because an old generation and `--no-rebuild` keep old artifacts reachable on purpose. The argv guard in `nix/runner.sh` stays as the belt for a hand-invoked runner.
5. Migrate existing project state: a retained lock carrying a dead `vivarium` node is regenerated or pruned with a stderr note, never silently, and the `nixpkgs` and `microvm` pins in it do not move.
6. Amend the owning documents in the same change: [`spec/02`](../../../reference/spec/02-config-and-xdg-layout.md) for what the lock contains, [`spec/04`](../../../reference/spec/04-composition-and-determinism.md) for the tool version becoming a declared build input, [`spec/10`](../../../reference/spec/10-vm-lifecycle.md) for the pre-boot refusal, and [`configuration-and-composition.md`](../../../explanation/configuration-and-composition.md) for the composition story. Delete the developer guide's stale-self-pin section outright rather than editing it: the failure it teaches a workaround for cannot occur once no lock names the tool.
7. Add the trial that proves the refusal: build, move the tree so the contract schema changes, and show the next session verb refuses before boot with both numbers named and that `--rebuild` clears it.

## Out of scope

- Implementing `viv update`; it stays `Designed` and this slice must not need it.
- Any change to how `nixpkgs`, `microvm`, or artifact-declared inputs resolve, lock, or override.
- Release packaging and installation channels for `viv` itself.
- Guest agent transport or handshake changes.
- A store collection as part of the host sweep.

## Governed by

- [`../../../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) — fixes that the installation supplies vivarium and the tool is never its own input; this slice enacts it.
- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — owns the lock's contents and ownership.
- [`../../../reference/spec/04-composition-and-determinism.md`](../../../reference/spec/04-composition-and-determinism.md) — owns the determinism guarantee this slice amends.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — owns ensure-running, staleness, and `--rebuild`.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — owns `78` as the refusal's category.
- [`../../../explanation/configuration-and-composition.md`](../../../explanation/configuration-and-composition.md) — owns the composition topology.
- [`../../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md) — fixes the regeneration seam the embedded tree rides.
- [`../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — fixes that no ordinary build moves a pin, which is why the input must go rather than be refreshed.

## Acceptance

When `viv` prepares a project, the generated flake SHALL declare no `vivarium` input and the effective lock SHALL carry no node for one.

When a launch runs, every host-side vivarium program SHALL come from the running installation and none from the project's build.

If a selected build's launch contract schema differs from the running binary's, the command SHALL refuse before boot, naming both versions and the remedy.

If a launch or boot record written by another version is read, the diagnostic SHALL name both schema versions and SHALL be distinct from the corruption case.

When a retained lock from the prior shape is met, the next preparation SHALL shed its `vivarium` node with a stderr note and SHALL NOT move any other pin.

While this slice's live verification runs, it SHALL run against a host swept of the prior shape's state, with the sweep's enumeration recorded before anything was removed.

## Rabbit holes

- The sweep is treated as a decision to re-open — escape: [`ADR-0102`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) is `Accepted`; the sweep reports what it removed and never asks whether the tool should supply itself.
- Embedding drags the verification tree along — escape: embed the product subtree only; `scripts/check-flake-boundary` and `scripts/check-verification-boundary` already draw the line.
- The migration invites rewriting locks in place — escape: shed the dead node or regenerate the file, never edit surviving pins; ADR-0059 owns why.
- The host sweep drifts into a store collection — escape: the sweep touches only vivarium-owned state; Q-023 owns the price of collecting and the operator owns the choice.
- Embedding many files invites a build-script framework — escape: one seam, the same shape the two already-embedded Nix files use, even if it is a generated list.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, [`ADR-0102`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) reaches `Implemented` with its enactment linked, the host sweep's before-and-after is recorded here, and the [`milestones.md`](../../milestones.md) row flips to `done` with slice 005 next.

## Revisions

Recorded 2026-08-17, before the slice started. `Q-014` and `Q-027` left [`../../open-questions.md`](../../open-questions.md) by direct decision rather than through this slice's work, and [`ADR-0102`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) is their single exit. Neither was a question in the sense that file keeps: a program depending on a built copy of itself is not a trade-off to weigh, and an installation is sufficient to supply the tool by definition. `Q-014` had framed the choice as branch-versus-pin and `Q-027` as what detects the resulting skew, which is why both read as open — they described the flawed shape's symptoms from two sides instead of naming the shape. Slices 012 and 014 link both by name; those records are frozen and correct as history, and this paragraph is where the forward reader learns where the two went. The pre-boot refusal survives the closure on its own merit, because an old generation and `--no-rebuild` keep old records reachable by design.
