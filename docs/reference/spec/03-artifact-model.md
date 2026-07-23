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

## Relationship

Images vary by toolchain, pieces by cross-cutting concern, manifests by the per-environment
combination. A project points at exactly one manifest, which resolves to exactly one image plus its
ordered pieces.
