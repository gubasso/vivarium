# Own setup at build

Record what a user can put in the guest, and what they can execute of their own, while it is being produced. What runs at each start is [the next row](./own-setup-at-start.md); whether the result reproduces for a second person is [the same-definition row](./same-definition.md).[^read]

1. Pick a program absent from the default environment, and a setup step the tool does not provide.
2. Add both by the tool's documented mechanism.
3. Record which file each request goes in, whether it ran, and as whom.

## vivarium

Partial, and the line is deliberate: any program goes in, no arbitrary command runs. A package is named in an image or a piece — both NixOS modules — and the module system concatenates those lists across every layer, so adding a piece adds its packages without touching the manifest that imported it. The place to write it is not the manifest: [`03-artifact-model.md`](../../spec/03-artifact-model.md)'s key table is the manifest's whole surface and carries no package key, so a personal one-off goes through `extends = "./custom.nix"` and anything meant to be reused becomes a piece.

What has no slot at all is a script, because a build step running as root is refused — read this beside [The build runs no user-supplied commands as root](./build-steps.md), which is the same refusal stated from the other side. Setup is expressed as declaration instead: an option to set, or a derivation that produces the file. Most setup hooks convert; one that expects to reach the network mid-build does not, because that is the reproducibility [the same-definition row](./same-definition.md#vivarium) measures. Not planned, and the cost is real.

Adding a test runner to one project, end to end — a piece in the config root, named by the manifest, resolved on the next start:

```nix
# pieces/rust-toolchain/default.nix
{ pkgs, ... }:
{ environment.systemPackages = [ pkgs.cargo-nextest ]; }
```

```toml
# manifests/rust-web.toml — the manifest names the piece, never the package
image = "dev-base"
pieces = ["rust-toolchain"]
```

```bash
viv start   # evaluates every layer, builds what changed, boots with the tool inside
```

## flake-pilot

Yes, in the image description rather than in the registration. A package is a name in the KIWI description the user builds the guest from, and a setup step is KIWI's [`config.sh`](https://github.com/OSInside/flake-pilot/blob/920f41e/appstore/firecracker/claude/config.sh), which runs arbitrary commands as root while the image is built — upstream's own example VM uses it to install its agent with `npm install -g` and to pipe a vendor installer into `bash`. The registration carries neither: no key in it or in an `<app>.d/` drop-in names a package, and `--include-tar` and `--include-path` copy a built payload onto the instance at provisioning rather than resolving a name or executing anything.

The same two additions, in the description upstream ships ([`appliance.kiwi`](https://github.com/OSInside/flake-pilot/blob/920f41e/appstore/firecracker/claude/appliance.kiwi), [`config.sh`](https://github.com/OSInside/flake-pilot/blob/920f41e/appstore/firecracker/claude/config.sh)):

```xml
<!-- appliance.kiwi, in the <packages type="image"> section -->
<package name="ripgrep"/>
```

```bash
# config.sh — runs as root inside the image while it is built
zypper --non-interactive addrepo https://download.opensuse.org/repositories/devel:tools/openSUSE_Tumbleweed/ devel-tools
```

Then rebuild and re-pull. Both commands, with the boxed KIWI invocation in full, are [the guest OS row](./guest-os.md#flake-pilot), which owns them; the registration itself does not change, because it never named either file.

The cost is that the description is a separate artifact from the registration, and nothing records which one produced a registered image. It is published all the same, in upstream's appstore. What [the same-definition row](./same-definition.md#flake-pilot) charges is the next step: a description naming moving repositories and unversioned packages does not rebuild to the image it first produced.

The `krun` route relocates the same answer into an OCI build: packages are lines in a `Containerfile` and setup is its `RUN` steps, run as root by whatever built the image, and the registration names neither. The cost is the same cost in a sharper form, because at that route the registration names a tag rather than an artifact, so nothing records which build the setup ran in.

## glaipnir

Yes, and this is the subject that has it most directly: a `PACKAGES=(…)` array is interpolated into the image's `zypper install` line, and `--build-hook` runs the user's script as root inside the build context — which is how a package outside the default Tumbleweed repositories gets its repository added first. The cost is [the no-root-build row](./build-steps.md#glaipnir) and [the same-definition row](./same-definition.md#glaipnir), where the same generality reads as a loss.

A package from the default repositories is the array alone; one from anywhere else needs the hook that adds its repository first, and the hook runs before the install line:

```bash
# glaipnir.conf
PACKAGES=(ripgrep fd jq)
```

```bash
# 10-add-devel-tools.sh — passed with --build-hook, run as root in the build context
zypper --non-interactive addrepo https://download.opensuse.org/repositories/devel:tools/openSUSE_Tumbleweed/ devel-tools
zypper --non-interactive --gpg-auto-import-keys refresh
```

## bunkerbox

Yes: the image config's `containerfile` is an ordinary container recipe, so a package and a setup step are both `RUN` lines, as root, unrestricted. Two other keys sit beside it — `build_args` passes values in, and `files` writes extra files into the build context — and the tool's own requirements on the recipe are narrow: a musl base, the two helper binaries copied in, the generated entrypoint, and `/workspace` and the home directory created.

The upstream pattern, with the version the one thing the line pins:

```dockerfile
FROM docker.io/library/alpine:3.22
ARG MY_TOOL_VERSION
RUN apk add --no-cache bash ca-certificates curl git \
      && curl -fsSL "https://example.com/my-tool-linux-musl.tar.gz" -o /tmp/my-tool.tar.gz \
      && tar -xzf /tmp/my-tool.tar.gz -C /usr/local/bin
```

What it costs is [the same-definition row](./same-definition.md#bunkerbox), where those same lines decide the version afresh on every rebuild.

[^read]: Read at `vivarium` `ceb0027`, `flake-pilot` `920f41e`, and `glaipnir` `21ef389` on 2026-08-20; `bunkerbox` `b7f14f3` on 2026-08-25.
