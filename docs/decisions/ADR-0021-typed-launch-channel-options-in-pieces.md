# ADR-0021: Typed launch-channel options in pieces

## Context and Problem Statement

ADR-0020 decided the mount/env schema and let pieces contribute, but left the piece-side surface
undefined: pieces are NixOS modules, not TOML, and it was unclear how a piece could carry the host
mounts its application needs without breaking the pure build (N3) or the personal-data-free rule
(N11). A piece should be the *whole* piece — packages, guest config, runtime env, and host mounts
for one concern, in one file — so adopting it never requires re-declaration.

## Considered Options

- Pieces stay guest-only; every mount is re-declared in each manifest
- Piece becomes a directory with a sidecar mounts file
- Typed `vivarium.*` NixOS options inside the piece module, extracted as launch data

## Decision Outcome

Chosen option: **typed `vivarium.*` options inside the piece module** — one file per concern,
natively type-checked, no second language and no translation layer.

- A tool-owned options module declares `vivarium.mounts` (`listOf` submodule `source`/`target`/
  `readonly`) and `vivarium.env` (`attrsOf str`); the module system validates shapes and merges
  lists/attrsets — no bespoke engine (N6). Manifest `[[mounts]]`/`[env]` compile into the same
  options.
- These options are **launch-channel data**: the tool reads `config.vivarium.*` by pure evaluation
  and applies it at launch; no build output may depend on it (new invariant N19). Host-side
  `${VAR}` expansion happens only at launch (N5).
- **Portable variables only in shared layers:** mount sources in shared pieces and manifests may
  use `${HOME}` and `${XDG_*}` — unexpanded, machine-independent — never literal personal paths;
  validation rejects literals before the build. Literal paths remain allowed in the
  personal/machine-local layer (N11, refined).

## Consequences

- Good: self-contained pieces — adopting a piece brings its mounts and env with it.
- Good: native type errors and module-system merge; the only bespoke check is the literal-path
  validation.
- Bad: reading launch data costs a pure evaluation per start; piece authors write Nix module
  syntax.

## Status

Accepted

Amends [`ADR-0020-mount-and-config-mirroring-schema.md`](ADR-0020-mount-and-config-mirroring-schema.md).
Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md),
[`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md),
[`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md),
[`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md), and
[`../reference/spec/08-invariants-and-guarantees.md`](../reference/spec/08-invariants-and-guarantees.md).
