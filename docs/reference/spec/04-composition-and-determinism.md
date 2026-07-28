# 04 — Composition and determinism

How the layers named by a manifest merge into one configuration, and what makes the resulting VM reproducible. The decision to reuse the module system rather than build a merge engine is in [`../../decisions/ADR-0002-module-system-as-composition-engine.md`](../../decisions/ADR-0002-module-system-as-composition-engine.md).

## Composition is module merge

A manifest resolves to an image plus an ordered list of pieces (see [`03-artifact-model.md`](./03-artifact-model.md)). The tool imports them as NixOS modules and the module system merges them. There is no separate vivarium merge engine.

The merge is **priority-based, not order-based**:

- **Lists concatenate.** Every layer that contributes to a list — package sets, mount lists, the egress allowlist — adds to it; the effective value is the union of all layers.
- **Scalars resolve by priority.** A scalar set with `mkDefault` yields to a normally-set scalar, which yields to one set with `mkForce`. Position in the `pieces` list does not decide the winner; priority does.

## The three-tier priority convention

vivarium assigns roles to priorities so composition is predictable:

- **Base defaults** — images set overridable values with `mkDefault`.
- **Project leaf** — the manifest's own settings use normal priority and so override base defaults.
- **Hard floor** — pieces that must not be overridden (for example a security policy) use `mkForce`.

This gives the ergonomics of layered overrides — "the project overrides the base, the security floor overrides everything" — without any custom ordering logic. `viv config eval` (see [`01-command-surface.md`](./01-command-surface.md)) renders the merged result so users can see the effective configuration; `viv config sources` shows which layer each value came from.

## The build and launch channels

The merged configuration divides into two channels, decided in [`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md):

- **Build channel** — everything the guest system derivation depends on: packages, services, policy. This is what `nix build` realizes into the immutable image.
- **Launch channel** — runtime declarations under the tool-owned `vivarium.*` options (`vivarium.mounts`, `vivarium.env`); the manifest's `[[mounts]]`/`[env]` tables compile into the same options. The tool reads this channel by **pure evaluation** of the merged configuration and applies it when the VM launches; no build output may depend on it (N19, [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)).

The split is what lets a shared piece carry host-facing declarations without breaking purity: host-side variables in mount sources stay unexpanded through evaluation and resolve against the host environment only at launch (see [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). Typing lives in the options module: `vivarium.mounts` and `vivarium.env` are declared with module option types, so a malformed declaration fails evaluation with a precise error rather than at boot.

## Determinism

A sandbox is a Nix build, and its inputs are pinned by a lockfile. Given the same manifest closure and the same lockfile, the build evaluates to the same store output on any machine and at any later time. Two consequences follow:

- **The store output hash is the freshness key.** Identical inputs produce an identical output path; the tool does not compute a separate content digest. A changed layer changes the inputs, which changes the output path, which triggers a rebuild. Each such output the tool retains is pinned as a **generation** ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)).
- **The build must be pure.** No host-specific value may enter it. In particular, the working directory path is injected at launch time, never built in, per [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md).

The determinism and purity requirements are stated normatively in [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md).
