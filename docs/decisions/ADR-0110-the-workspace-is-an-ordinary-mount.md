# ADR-0110: The workspace is an ordinary mount at its own path

## Context and Problem Statement

[`ADR-0009-launch-time-workspace-path-injection.md`](./ADR-0009-launch-time-workspace-path-injection.md) keeps the working directory's host path out of the build; N5 and N16 carry the rule. Each share mounts at a build-time constant while the real path crosses on the kernel command line into its own guest unit. [`ADR-0108-a-workspace-is-owned-by-one-manifest.md`](./ADR-0108-a-workspace-is-owned-by-one-manifest.md) retired the question it answered. A workspace is declared in the manifest now; that manifest lands in the store already (N10), and its `source` already reaches the build as the contract's `sourceToken`. The channel now buys one property: a `${HOME}/x` source yields one build usable under two different `$HOME` values.

## Considered Options

- Keep the launch-time bind above a build-time constant.
- Compile a workspace to an ordinary declared mount whose `target` is its expanded `source`.
- Keep the share family and delete only the guest unit.

## Decision Outcome

Chosen option: `an ordinary declared mount` — the option [`ADR-0100-the-workspace-mirrors-its-host-path.md`](./ADR-0100-the-workspace-mirrors-its-host-path.md) rejected while the path was unknowable at build time, and the one that leaves a single sharing mechanism now it is declared.

Rust expands each `[[workspaces]]` row, refuses it against the paths the guest owns and its siblings, then emits `source` and `target` as one literal path. Nix keeps no workspace concept: no option, no tag family, no internal root, no mirror unit. Ownership is untouched, decided from manifest text and refused at `78` as [`ADR-0109-an-undeclared-working-directory-is-refused.md`](./ADR-0109-an-undeclared-working-directory-is-refused.md) put it.

## Consequences

- Good: one sharing mechanism and one guest unit; the encoder, the tag family, and the command-line channel are deleted.
- Good: the guest-owned and nesting refusals move to resolution, naming host paths rather than encoded ones, on every verb alike.
- Good: `/proc/cmdline` no longer discloses the host's directory layout to guest processes.
- Bad: the expanded path enters the build, so one manifest evaluates to different store outputs on two machines, and moving a tree rebuilds.
- Bad: a layer mounting over a workspace is refused as a duplicate target rather than as an ownership violation, and one written against the deleted option is ignored rather than refused (`Q-034`).

## Status

Implemented — the expansion and its refusals are in [`../../src/launch/workspace.rs`](../../src/launch/workspace.rs) and [`../../src/cli/mod.rs`](../../src/cli/mod.rs), the compiled form in [`../../src/config/flake.rs`](../../src/config/flake.rs), and the one bind unit in [`../../nix/mount-bind.sh`](../../nix/mount-bind.sh). Enacted by [slice 029](../plan/slices/029-a-workspace-is-just-a-mount/README.md).

Supersedes [`ADR-0009-launch-time-workspace-path-injection.md`](./ADR-0009-launch-time-workspace-path-injection.md), whose decision this reverses outright: the path it kept out of the build is now a build input, and the determinism it bought is retracted in favour of deleting the channel that bought it.

Supersedes [`ADR-0100-the-workspace-mirrors-its-host-path.md`](./ADR-0100-the-workspace-mirrors-its-host-path.md) on mechanism. Its outcome stands and moves here unchanged: every declared tree is reachable inside the guest at the absolute path it occupies on the host, and a path equal to, containing, or under one the guest owns is refused. What it replaces is the launch-time bind above a build-time constant, and with it that record's second and third `Bad` consequences.

Amends [`ADR-0020-mount-and-config-mirroring-schema.md`](./ADR-0020-mount-and-config-mirroring-schema.md) — a workspace now compiles into its schema, so the schema gains a row whose `target` equals its `source` and whose expanded path is a build input rather than launch data.

Amends [`ADR-0021-typed-launch-channel-options-in-pieces.md`](./ADR-0021-typed-launch-channel-options-in-pieces.md) and [`ADR-0041-resource-and-volume-channel-classification.md`](./ADR-0041-resource-and-volume-channel-classification.md) — a declared workspace joins `vivarium.volumes` on the build-channel side of the per-option classification, which is the third application of that rule rather than an exception to it.

Amends [`ADR-0108-a-workspace-is-owned-by-one-manifest.md`](./ADR-0108-a-workspace-is-owned-by-one-manifest.md) — the authored `[[workspaces]]` table and its single-valued ownership stand exactly as decided; what changes is that the table compiles to a mount row rather than to a share family of its own, so "not a `[[mounts]]` row" describes the surface a user writes rather than the merged configuration.

Retires N5 and amends N3, N10, N16, and N19 in [`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md).
