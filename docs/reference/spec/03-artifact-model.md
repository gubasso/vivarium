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

[egress]                                            # optional; policy default, see below
mode  = "open"
allow = [ ]

[env]                                               # optional; guest environment
RUST_BACKTRACE = "1"

[[mounts]]                                          # optional; repeatable
source   = "${HOME}/.config/foo"                    # host path; ${VAR} expands at launch
target   = "~/.config/foo"                          # guest path; ~ is the guest home
readonly = true                                     # default false

[[volumes]]                                         # optional; repeatable
name     = "cache"
mount    = "/var/cache/project"
size_gib = 64                                       # optional; a virtual ceiling, not a reservation

# Escape hatch for composition the TOML cannot express. Naming it requires
# the directory manifest form, so this file is manifests/rust-web/default.toml
# and the module sits beside it:
# extends = "./custom.nix"
```

### The key table

This is the complete authoring surface. Nothing outside it is accepted, and an unknown key fails closed naming the accepted keys and the CLI version — the manifest carries **no schema version**, because the grammar evolves additively and a key's meaning is never repurposed ([`../../decisions/ADR-0047-manifest-carries-no-schema-version.md`](../../decisions/ADR-0047-manifest-carries-no-schema-version.md)). The table and the validation boundary below it are decided in [`../../decisions/ADR-0057-manifest-grammar-and-validation-boundary.md`](../../decisions/ADR-0057-manifest-grammar-and-validation-boundary.md).

| Key                    | Type                                               | Required | Default        | Channel |
| ---------------------- | -------------------------------------------------- | -------- | -------------- | ------- |
| `image`                | name                                               | **yes**  | —              | build   |
| `pieces`               | array of names                                     | no       | `[]`           | build   |
| `extends`              | relative path to a `.nix` module                   | no       | `null`         | build   |
| `resources.mem_mib`    | integer ≥ 256                                      | no       | host-resolved  | launch  |
| `resources.vcpu`       | integer ≥ 1                                        | no       | host-resolved  | launch  |
| `egress.mode`          | `"open"` \| `"allowlist"`                          | no       | policy default | build   |
| `egress.allow`         | array of host names                                | no       | `[]`           | build   |
| `env.<NAME>`           | string, `NAME` matching `^[A-Za-z_][A-Za-z0-9_]*$` | no       | `{}`           | launch  |
| `[[mounts]].source`    | host path; `${VAR}` stays unexpanded               | in table | —              | launch  |
| `[[mounts]].target`    | guest path; `~` is the guest home                  | in table | —              | launch  |
| `[[mounts]].readonly`  | boolean                                            | no       | `false`        | launch  |
| `[[volumes]].name`     | name; `default` is reserved                        | in table | —              | build   |
| `[[volumes]].mount`    | absolute guest path                                | in table | —              | build   |
| `[[volumes]].size_gib` | integer ≥ 1                                        | no       | policy default | launch  |
| `[volume].size_gib`    | integer ≥ 1                                        | no       | policy default | launch  |
| `[volume].persist`     | array of absolute guest paths                      | no       | `[]`           | build   |

A **name** is the kebab-case identifier resolved against the config library ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). "In table" means the key is required once its `[[…]]` entry exists, not that the table itself is required — every table here is optional. The **channel** column is the purity classification: a launch-channel value is read by pure evaluation and applied when the VM boots, and no build output may depend on it ([`04-composition-and-determinism.md`](./04-composition-and-determinism.md), [`../../decisions/ADR-0041-resource-and-volume-channel-classification.md`](../../decisions/ADR-0041-resource-and-volume-channel-classification.md)). An absent table is never the same as an empty one: an undeclared ceiling resolves from the host at launch rather than to zero ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)).

The **Default** column prints a literal only where this page owns it — the structural empties, and `readonly`, whose absence-means-false is grammar rather than policy. Where the value is a policy another page decides, the cell reads _policy default_ or _host-resolved_ and that page states the number: the egress default in [`05-networking-and-egress.md`](./05-networking-and-egress.md), volume size and the resource ceilings in [`17-resources-and-capacity.md`](./17-resources-and-capacity.md). Restating those figures here would put a second authority on a value that can move.

Two conventions the table encodes. **Units live in key names** — `mem_mib`, `size_gib` — never in value suffixes, so there is no scale to disambiguate and no suffix grammar to learn. And **there is no `resources.disk`**: disk belongs to a volume, which already declares and reports its own ceiling. `[volume]` and `[[volumes]]` are deliberately different names because TOML forbids a table and an array of tables sharing one; the singular table configures the default home volume, which always exists without declaration.

The `[[mounts]]` and `[env]` shapes are owned by [`../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../decisions/ADR-0020-mount-and-config-mirroring-schema.md), the volume keys by [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md).

The `pieces` list is ordered, but order never decides a scalar's value: merge is priority-based, and two layers setting one scalar at the same priority is a content defect that fails evaluation rather than an order-resolved win (see [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). Order is preserved because it is what readers follow and because concatenated lists keep it.

### Validation: what fails when

A defect is reported at exactly one stage, under exactly one code:

| Stage    | Defect                                                                                                                                                   | Exit |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| parse    | malformed TOML; an unknown key; a wrong type; a value outside its domain (`mem_mib = 0`, `mode = "off"`, a non-kebab name); `extends` in a flat manifest | `78` |
| resolve  | a name with no matching file, or both spellings of one name present ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md))                     | `78` |
| evaluate | one volume name bound to two mountpoints, a duplicate mount target, an equal-priority scalar tie, a literal personal path in a shared layer (N11)        | `65` |

The dividing line is **what the manifest text alone can decide**. Everything else waits for the merge — including the duplicate-name and duplicate-target checks, which a single manifest can violate on its own. They still run only at evaluation, because a piece can contribute the colliding declaration and one defect must never carry two exit codes — the codes are a permanent API ([`14-exit-codes.md`](./14-exit-codes.md), [`../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../decisions/ADR-0042-evaluation-time-content-defects.md)).

### `extends`

`extends` names **one raw `.nix` module** — the escape hatch for composition the TOML cannot express. Its semantics are fixed in [`../../decisions/ADR-0060-extends-is-one-local-module.md`](../../decisions/ADR-0060-extends-is-one-local-module.md):

- **Exactly one value, never an array and never transitive.** A Nix module already has `imports`, which the module system evaluates; a list vivarium ordered itself would make declaration order decide again.
- **It requires the directory manifest form.** A manifest naming `extends` must resolve as `manifests/<name>/default.toml`, and a flat manifest that names it is `78` at parse, naming the required spelling — one `mv` resolves it, the same remedy the both-spellings ambiguity gets ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). This is what makes the bullet below safe, and it is why the rule exists ([`../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md`](../../decisions/ADR-0063-extends-requires-the-directory-manifest-form.md)).
- **Resolved relative to the manifest file's own directory**, and the result must canonicalize — after symlinks — inside that same directory. An escape, a missing file, or a non-module target is `78`.
- **That directory is copied into the generated flake**, so its own relative imports work and the module is a build input like any other layer (N3, [`04-composition-and-determinism.md`](./04-composition-and-determinism.md)). The unit is `manifests/<name>/` and nothing else: the rest of the manifest library stays excluded, so an unrelated manifest can never change this project's output path (N4).
- **It merges at the same rank as a piece** and chooses its own priority — `mkDefault` to propose, `mkForce` to override a floor, which is what the tie hint in [`01-command-surface.md`](./01-command-surface.md) means by "override through extends".

`extends` does **not** inherit another manifest. Manifest-to-manifest inheritance would require vivarium to publish, per key, whether a child value replaces or merges with its parent's — a merge engine of its own, which N6 forbids. A live team baseline is a shared **piece** instead: a piece carries packages, guest config, mounts, env, resources, and volumes, and may import an image, so adopting it brings the whole baseline and editing it reaches every teammate's next build ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)).

### A manifest is compiled, so its text lands in the store

The manifest does not merely name layers that get built — it is itself translated into a module in the generated flake, and that flake is realized by Nix ([`../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md)). Every value written here therefore reaches the world-readable store, including the launch-channel tables. That is not a breach of N19: no build output depends on a launch-channel value, but the text declaring it is copied to the store like the rest of the manifest. Nothing secret belongs in a manifest — the rule and the channels that replace it are in [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md) (N10).

## Authoring these artifacts

vivarium never scaffolds these files into a user's config root (N13). Instead, the manifest's TOML surface is documented by **generated, self-documented examples** derived from the tool's own config types — an annotated `*.example.toml` plus a JSON Schema for editor validation — kept in sync by a pre-commit check. The user copies an example and edits it; the tool only ever reads the result. The inline sketches above are illustrative; the canonical, always-current examples are the generated ones. The `vivarium.*` options available to pieces are declared by a tool-owned options module and follow the same rule — their reference documentation is generated from the option types. See [`../../decisions/ADR-0012-generate-config-examples-from-types.md`](../../decisions/ADR-0012-generate-config-examples-from-types.md).

What vivarium ships alongside those generated examples is **hand-maintained example images and pieces to copy** — and nothing more. No bundled artifact resolves by name: the config root is the entire search path, so a name has exactly one meaning and no shadowing rule joins resolution ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), [`../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md`](../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md)). The tool-owned options module is the one thing that is not an example: it is non-optional, ships inside the generated flake, and is never copied into a library or listed by one.

## Relationship

Images vary by toolchain, pieces by cross-cutting concern, manifests by the per-environment combination. A project points at exactly one manifest, which resolves to exactly one image plus its ordered pieces. Images and pieces are the shared surface a team distributes; the manifest is each user's own, so two people on one project adopt the same pieces through manifests that need not match.
