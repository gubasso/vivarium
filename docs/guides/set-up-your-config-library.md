# Set up your config library

Use this guide the first time you fill your config root, and again whenever a new host file needs to reach a sandbox. It walks one worked library — a global file, an image, four pieces, and two manifests — and it puts the mounts where they belong: with the piece whose concern needs them, so adopting the piece brings its host files along.

Everything here runs today. The commands that do not yet exist are marked where they appear, and [`../reference/implementation-status.md`](../reference/implementation-status.md) is the source of truth for that. The facts this guide only puts in order are owned elsewhere: the roots and the library layout by [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), the artifact shapes by [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), the mount schema by [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and [ADR-0020](../decisions/ADR-0020-mount-and-config-mirroring-schema.md), and the sharing rule by [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).

## The library you are about to write

Everything lives under one root, and vivarium only ever reads it — the tool scaffolds nothing here, so every file below is one you create (N13):

```text
$XDG_CONFIG_HOME/vivarium/
├── config.toml              # cross-cutting defaults for every command
├── images/
│   └── rust.nix             # the toolchain layer
├── pieces/
│   ├── git-identity.nix     # one concern, with the host files it needs
│   ├── gh-cli.nix
│   ├── main-repo.nix
│   └── egress-crates.nix
└── manifests/
    ├── rust-web.toml        # one sandbox and its declared workspaces
    └── rust-web-offline.toml
```

`config.toml` carries the settings that apply to every command rather than to one sandbox — today the logging family ([`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md), precedence in [ADR-0046](../decisions/ADR-0046-global-config-file-and-precedence.md)). Workspace ownership belongs in each personal manifest, while the derived cache remains machine-local and rebuildable.

Each library resolves a bare name to one file, flat form first and directory form second — `pieces/git-identity.nix`, or `pieces/git-identity/default.nix` when the piece grows a directory ([ADR-0045](../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md)). Reach for the directory form when a piece needs an `inputs.toml` or files beside it; nothing else changes.

## Step 1 — The image: what the toolchain is

An image is a NixOS module and the base layer. It answers "what is installed", and nothing about your host:

```nix
# images/rust.nix
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [
    rustup
    cargo
    rust-analyzer
    gcc
    pkg-config
  ];
}
```

## Step 2 — Pieces: one concern, and the host files that concern needs

A piece is the whole piece for its concern. Beyond guest config it declares what its application needs through the tool-owned options — `vivarium.mounts` for host files, `vivarium.env` for runtime environment, `vivarium.credentials` for an agent channel — so adopting the piece needs no re-declaration in anyone's manifest. The option surface is supplied by the generated flake, so a piece imports nothing to use it.

Two rules shape every mount below, and both are worth understanding before writing the first one.

A source is a host path whose `${VAR}` stays unexpanded through evaluation and resolves against your host environment at launch. That is what makes a piece shareable: `${HOME}` means a different directory for each teammate, and a literal `/home/ana/…` in a shared image or piece is refused at evaluation with `65` (N11). A target is a guest path where `~` expands to the guest home — so an identity mount needs no translation even though the guest username differs from yours.

Declarations concatenate across layers, and two layers declaring the same target fail evaluation. Pick targets that name the concern.

### A directory mount: git identity

```nix
# pieces/git-identity.nix
{
  vivarium.mounts = [
    {
      source = "\${HOME}/.config/git";
      target = "~/.config/git";
      readonly = true;
    }
  ];
}
```

The backslash is Nix, not a typo: it keeps `${HOME}` a literal string rather than a Nix interpolation, so the host expands it at launch. `readonly = true` is what identity deserves — the guest reads your git config and can never rewrite it, and read-only is enforced on both sides, with the guest mount `ro,nodev,nosuid,noexec`.

### A file mount, and the one thing to know about it

```nix
# pieces/gh-cli.nix
{ pkgs, ... }:
{
  environment.systemPackages = [ pkgs.gh ];

  vivarium.mounts = [
    {
      source = "\${HOME}/.config/gh/hosts.yml";
      target = "~/.config/gh/hosts.yml";
      readonly = true;
    }
  ];

  vivarium.env.GH_CONFIG_DIR = "~/.config/gh";
}
```

A regular-file source serves that file and nothing else: the launcher stages it as the only entry of that share's own export root, so no sibling in `~/.config/gh` is reachable through this share by any guest process, guest root included ([ADR-0105](../decisions/ADR-0105-a-file-mount-is-staged-into-its-own-export-root.md)). What you give up is name replacement rather than content: a mount conveys an inode, so a host editor that saves by writing a temporary file and renaming it over the source leaves the guest reading the old inode until the next boot. Where the file changes that way while the sandbox runs, declare the directory instead and accept that the whole directory crosses.

### A second workspace: the repository your worktree points at

A linked git worktree records an absolute path back to its main repository, so the main repository has to be reachable at that same path or `git` resolves it from one side only:

```toml
# manifests/api-worktree.toml
[[workspaces]]
source = "${HOME}/src/api"

[[workspaces]]
source = "${HOME}/src/api-feature"
```

Workspace ownership is the point here — both trees select the same sandbox, mirror at their host paths, and are read-write. Every declared share gets its own confined virtiofsd process with only that share's host path in its view, so neither can reach the other or the rest of your home through its share.

### A piece with no mounts at all

Not every concern needs a host file, and a policy floor is the clearest case:

```nix
# pieces/egress-crates.nix
{ lib, ... }:
{
  sandbox.egress.mode = lib.mkForce "allowlist";
  sandbox.egress.allow = [ "static.crates.io" "index.crates.io" "github.com" ];
}
```

Use `mkForce` only for a floor that must hold for everyone. A piece that merely proposes a value uses `lib.mkDefault`, so an adopter's manifest can still decide — two pieces setting the same scalar at normal priority is an evaluation failure, and `viv config sources` prints it as a `[tie]` with the fix.

## Step 3 — The manifest: your own layer

The manifest is personal. It names one image, the ordered pieces, and the knobs you own:

```toml
# manifests/rust-web.toml
image  = "rust"
pieces = [ "git-identity", "gh-cli", "main-repo" ]

[resources]
mem_mib = 8192
vcpu    = 4

[env]
RUST_BACKTRACE = "1"

[[workspaces]]
source = "/home/alice/src/rust-web"

# A literal host path is legal here — a manifest is yours and travels nowhere.
[[mounts]]
source   = "/srv/fixtures/large-corpus"
target   = "/workspaces/corpus"
readonly = true

[[volumes]]
name     = "cargo-cache"
mount    = "/home/vivarium/.cargo"
size_gib = 32
```

The manifest's `[[mounts]]` and a piece's `vivarium.mounts` compile into the same merged list, so the two spellings are one channel and the choice between them is about ownership: a mount everyone adopting the concern needs belongs in the piece, and a mount only this project needs belongs here.

A second manifest reusing the same pieces is the whole payoff of putting mounts in them:

```toml
# manifests/rust-web-offline.toml
image  = "rust"
pieces = [ "git-identity", "gh-cli", "main-repo", "egress-crates" ]
```

Note what is not in this file: no repetition of the three mounts, because each rode in with its piece.

A cache belongs in a volume rather than a mount, as `cargo-cache` above is. A share is not a general-purpose local filesystem — it cannot create an unnamed temporary file and link it into place, cannot make device nodes, and carries no POSIX ACLs — and a toolchain that needs any of those wants a volume or guest-local scratch.

## Step 4 — Read the merge before booting

```console
$ cd /home/alice/src/rust-web
$ viv config eval
$ viv config sources
```

`config eval` renders the merged configuration in the same TOML shape you authored, so every mount that will exist at launch is on one screen. `config sources` names the layer each value came from — which is the command that answers "why is this mount here" when a piece contributed it.

Then boot and look from the inside:

```console
$ viv start
$ viv exec -- ls -A ~/.config/gh
$ viv exec -- git -C /workspaces/api status
```

## What fails, and when

Mount faults split by what decides them, which is why they carry two different codes:

| Declaration                                                        | When it fails             | Code |
| ------------------------------------------------------------------ | ------------------------- | ---- |
| a literal personal path in a shared image or piece (N11)           | evaluation, from the text | `65` |
| a non-portable variable in a shared image or piece's source        | evaluation, from the text | `65` |
| two layers declaring the same `target`                             | evaluation                | `65` |
| a literal `/tmp`, `/var/tmp`, or `${XDG_RUNTIME_DIR}` source (N24) | evaluation, from the text | `65` |
| a source whose variable is unset on this host                      | launch, before boot       | `78` |
| a source that does not exist                                       | launch, before boot       | `78` |
| a source that is a socket, FIFO, or device node                    | launch, before boot       | `78` |
| a source that expands into a session directory                     | launch, before boot       | `78` |

The split is not arbitrary: what the text alone decides is refused as a content defect, and what only host expansion reveals is refused at launch — before anything boots, either way. A socket is refused rather than mounted because a share conveys an inode and not a listener; forwarding an agent is [its own channel](./keep-secrets-out-of-the-store.md).

## Take the base: own what the guest is built from

Everything above chose what goes into the guest; the base flake chooses what all of it is built from. vivarium ships two references compiled into the binary — a `nixpkgs` branch and `microvm.nix` — and a project that never says otherwise builds from them. To own them, give your image the directory form and put a `flake.nix` beside its module:

```console
$ cp -r examples/images/base "${XDG_CONFIG_HOME:-$HOME/.config}/vivarium/images/"
$ $EDITOR "${XDG_CONFIG_HOME:-$HOME/.config}/vivarium/images/base/flake.nix"
```

That file is a real flake, and it is yours: point `nixpkgs` at a release channel, a fork, or a store path; declare anything else your image wants and reach it from `default.nix` as `vivariumInputs.<image-name>.<attr>`. The copy stops tracking vivarium's example the moment it lands in your config root — nothing resolves to `examples/` ([`ADR-0061`](../decisions/ADR-0061-examples-ship-not-a-second-namespace.md)) — and no vivarium release sits between you and a component fix any more: when the fix is on your reference, `viv update` moves your pin and the next `viv start` boots it ([`ADR-0112`](../decisions/ADR-0112-the-selected-image-carries-the-base-flake.md)).

Three facts keep the model straight. The file's text is a layer: edit it and the next `start` rebuilds with the edit, like any other layer edit. Its references are pins: what `nixpkgs` resolves to moves only under `viv update`, reported before and after, never during an ordinary build. And its URLs land in the generated flake, the lock, and the world-readable store — a reference embedding a credential is a build-time secret, exactly as a token in a manifest is (N10, [`keep-secrets-out-of-the-store.md`](./keep-secrets-out-of-the-store.md)).

## Where to go next

- Put a credential in a sandbox without putting it in the store: [`keep-secrets-out-of-the-store.md`](./keep-secrets-out-of-the-store.md).
- Share these pieces with a team while each person keeps their own manifest: [`team-shared-personal-overrides.md`](./team-shared-personal-overrides.md).
- Narrow what the sandbox may reach on the network: [`restrict-egress-allowlist.md`](./restrict-egress-allowlist.md).

## Acceptance coverage

The base handoff is covered by `workflow_18_update_usage_surface` in [`local_workflows.rs`](../../tests/local_workflows.rs) and `workflow_18_update_moves_the_pin` in [`eval_workflows.rs`](../../tests/eval_workflows.rs), and end to end — a redirected base, a moved pin, and the booted guest carrying the moved component — by [`tests/host/update-check`](../../tests/host/update-check). The composition this guide walks is covered by `workflow_03_team_shared_and_personal_override` (a shared piece carrying a mount, adopted by two manifests) and `workflow_17_declared_mounts_eval` (the `65` tier above) in [`eval_workflows.rs`](../../tests/eval_workflows.rs), and by `workflow_17_declared_mounts_refusals` (the `78` tier), `workflow_17_declared_mounts_round_trip` (both kinds reach the guest, one daemon per share), `workflow_22_file_mount_serves_only_its_file` (a file share serves that file alone), and `workflow_17_linked_worktree_reaches_main` (the second-workspace piece) in [`boot_workflows.rs`](../../tests/boot_workflows.rs).
