# 03 — Artifact model: images, pieces, manifests

vivarium composes a sandbox from three artifact kinds. The rationale is in
[`../../decisions/ADR-0003-images-pieces-manifests-model.md`](../../decisions/ADR-0003-images-pieces-manifests-model.md);
this page specifies their shapes. All three live under the config root (see
[`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)).

## Images

An **image** is a composable VM base, expressed as a NixOS module. Images capture the toolchain and
base system: a minimal base, a Rust toolchain, a Python toolchain, a GPU-enabled base. Images may
import one another, so a language image builds on a shared base.

An image sets soft defaults with `mkDefault` so pieces and manifests can override them (see
[`04-composition-and-determinism.md`](04-composition-and-determinism.md)). Example sketch of a Rust
image that layers onto a base:

```nix
{ pkgs, ... }:
{
  imports = [ ./base.nix ];
  environment.systemPackages = with pkgs; [ rustup cargo rust-analyzer gcc pkg-config ];
}
```

## Pieces

A **piece** is a small, single-purpose config fragment, also a NixOS module, layered onto an image.
Pieces capture cross-cutting concerns independent of the toolchain: git identity, ssh-agent
forwarding, cache mounts, or the egress policy. Pieces contribute to lists (packages, mounts,
allowlists) that concatenate across layers, and they set scalars at a priority appropriate to their
role. Example sketch of an egress-restriction piece:

```nix
{ lib, ... }:
{ sandbox.egress.mode = lib.mkForce "allowlist"; }
```

A piece is the *whole* piece for its concern. Beyond guest config, it may declare the runtime
mounts and environment its application needs through the tool-owned, typed `vivarium.*` options
(`vivarium.mounts`, `vivarium.env`) — launch-channel data, extracted by pure evaluation and never
a build input (see [`04-composition-and-determinism.md`](04-composition-and-determinism.md)). The
module system type-checks these declarations; shared pieces reference the host only through
portable variables (see [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md)).
Example sketch of a self-contained application piece, decided in
[`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md):

```nix
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [ foo ];

  # Launch-channel data: applied at launch, never built in. The backslash keeps
  # "${HOME}" unexpanded in Nix so the host resolves it at launch.
  vivarium.mounts = [
    { source = "\${HOME}/.config/foo"; target = "~/.config/foo"; readonly = true; }
  ];
  vivarium.env.FOO_CONFIG = "~/.config/foo";
}
```

Adopting a piece therefore brings everything the concern needs — packages, guest config, mounts,
and env — with no re-declaration in the manifest.

## Manifests

A **manifest** is the unifier and the single source of truth a project binds to. It names one image,
an ordered list of pieces, and policy knobs (resources, egress). It is authored as TOML and compiled
by the tool into a generated flake whose module `imports` are the named image and pieces, per
[`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md).
Example:

```toml
image  = "rust"
pieces = [ "git", "ssh-agent", "direnv", "egress-open" ]

[resources]
mem_mib = 4096
vcpu    = 4

[egress]
mode = "open"

# Escape hatch for composition the TOML cannot express:
# extends = "./rust-web.custom.nix"
```

The `pieces` list is ordered; because merge is priority-based rather than order-based, order matters
only where two layers set the same scalar at the same priority (see
[`04-composition-and-determinism.md`](04-composition-and-determinism.md)). The optional `extends` key
references a raw `.nix` module for advanced cases.

## Authoring these artifacts

vivarium never scaffolds these files into a user's config root (N13). Instead, the manifest's TOML
surface is documented by **generated, self-documented examples** derived from the tool's own config
types — an annotated `*.example.toml` plus a JSON Schema for editor validation — kept in sync by a
pre-commit check. The user copies an example and edits it; the tool only ever reads the result. The
inline sketches above are illustrative; the canonical, always-current examples are the generated ones.
The `vivarium.*` options available to pieces are declared by a tool-owned options module and follow
the same rule — their reference documentation is generated from the option types.
See [`../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../decisions/ADR-0012-generate-config-examples-from-types.md).

## Relationship

Images vary by toolchain, pieces by cross-cutting concern, manifests by the per-environment
combination. A project points at exactly one manifest, which resolves to exactly one image plus its
ordered pieces.
