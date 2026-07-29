# 03 — Artifact model: images, pieces, manifests

vivarium composes a sandbox from three artifact kinds. The rationale is in [`../../decisions/ADR-0003-images-pieces-manifests-model.md`](../../decisions/ADR-0003-images-pieces-manifests-model.md); this page specifies their shapes. All three live under the config root (see [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)).

## Images

An **image** is a composable VM base, expressed as a NixOS module. Images capture the toolchain and base system: a minimal base, a Rust toolchain, a Python toolchain, a GPU-enabled base. Images may import one another, so a language image builds on a shared base.

An image sets soft defaults with `mkDefault` so the user's manifest — or a piece that must not be overridden — can override them (see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). Example sketch of a Rust image that layers onto a base:

```nix
{ pkgs, ... }:
{
  imports = [ ./base.nix ];
  environment.systemPackages = with pkgs; [ rustup cargo rust-analyzer gcc pkg-config ];
}
```

## Pieces

A **piece** is a small, single-purpose config fragment, also a NixOS module, layered onto an image. Pieces capture cross-cutting concerns independent of the toolchain: git identity, ssh-agent forwarding, cache mounts, or the egress policy. Pieces contribute to lists (packages, mounts, allowlists) that concatenate across layers, and they set scalars at the priority their role calls for: `mkDefault` to propose a value the user's manifest may still override, `mkForce` only for a floor that must hold for everyone (see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). Pieces are **shared** artifacts, so a team's reproducibility and policy guarantees live here rather than in any one user's manifest ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). Example sketch of an egress-restriction piece, whose `mkForce` is exactly such a floor:

```nix
{ lib, ... }:
{ sandbox.egress.mode = lib.mkForce "allowlist"; }
```

A piece is the _whole_ piece for its concern. Beyond guest config, it may declare what its application needs through the tool-owned, typed `vivarium.*` options: `vivarium.mounts` and `vivarium.env` for runtime mounts and environment, `vivarium.resources` for a ceiling it proposes, and `vivarium.volumes` for a disk it needs. The first three are launch-channel data, extracted by pure evaluation and never a build input; `vivarium.volumes` is build-channel, because a volume's guest mountpoint is guest system configuration (see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). The module system type-checks these declarations; shared pieces reference the host only through portable variables (see [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)). Example sketch of a self-contained application piece, decided in [`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md):

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

Adopting a piece therefore brings everything the concern needs — packages, guest config, mounts, and env — with no re-declaration in the manifest.

## Manifests

A **manifest** is the unifier and the single source of truth a project binds to. It is the user's own artifact — the **personal** side of the sharing split ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)) — and it names one image, an ordered list of pieces, and policy knobs (resources, egress). It is authored as TOML and compiled by the tool into a generated flake whose module `imports` are the named image and pieces, per [`../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md`](../../decisions/ADR-0004-toml-manifest-compiles-to-flake.md). Example:

```toml
image  = "rust"                                     # required; names one image
pieces = [ "git", "ssh-agent", "direnv", "egress-open" ]   # optional; ordered

[resources]                                         # optional ceilings, not reservations
mem_mib = 4096
vcpu    = 4

[egress]                                            # optional; defaults to open
mode  = "open"
allow = [ ]

[env]                                               # optional; guest environment
RUST_BACKTRACE = "1"

[[mounts]]                                          # optional; repeatable
source   = "${HOME}/.config/foo"                    # host path; ${VAR} expands at launch
target   = "~/.config/foo"                          # guest path; ~ is the guest home
readonly = true                                     # default false

[[volumes]]                                         # optional; repeatable
name  = "cache"
mount = "/var/cache/project"

# Escape hatch for composition the TOML cannot express:
# extends = "./rust-web.custom.nix"
```

`image` is the only required key; every other table is optional, and an absent table is not the same as an empty one — an undeclared resource ceiling resolves from the host at launch rather than to zero ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)). The `[[mounts]]` and `[env]` shapes are owned by [`../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../decisions/ADR-0020-mount-and-config-mirroring-schema.md), `[[volumes]]` and `[volume].persist` by [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md). A manifest carries **no schema version**: the grammar evolves additively, and an unknown key fails closed rather than being ignored ([`../../decisions/ADR-0047-manifest-carries-no-schema-version.md`](../../decisions/ADR-0047-manifest-carries-no-schema-version.md)).

The `pieces` list is ordered, but order never decides a scalar's value: merge is priority-based, and two layers setting one scalar at the same priority is a content defect that fails evaluation rather than an order-resolved win (see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). Order is preserved because it is what readers follow and because concatenated lists keep it. The optional `extends` key references a raw `.nix` module for advanced cases.

## Authoring these artifacts

vivarium never scaffolds these files into a user's config root (N13). Instead, the manifest's TOML surface is documented by **generated, self-documented examples** derived from the tool's own config types — an annotated `*.example.toml` plus a JSON Schema for editor validation — kept in sync by a pre-commit check. The user copies an example and edits it; the tool only ever reads the result. The inline sketches above are illustrative; the canonical, always-current examples are the generated ones. The `vivarium.*` options available to pieces are declared by a tool-owned options module and follow the same rule — their reference documentation is generated from the option types. See [`../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../decisions/ADR-0012-generate-config-examples-from-types.md).

## Relationship

Images vary by toolchain, pieces by cross-cutting concern, manifests by the per-environment combination. A project points at exactly one manifest, which resolves to exactly one image plus its ordered pieces. Images and pieces are the shared surface a team distributes; the manifest is each user's own, so two people on one project adopt the same pieces through manifests that need not match.
