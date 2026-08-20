# Guest OS

Record which operating system runs inside, and whether the user can name a different one.[^read]

1. Read the tool's own definition format for a key naming a base system or image.
2. Set it to a second, unrelated distribution and rebuild.
3. Start the workload and record what `/etc/os-release` says inside.
4. Record what had to change on the host for that to work.

## vivarium

No, by rule: the guest is a NixOS system because vivarium [composes through the NixOS module system](../../spec/08-invariants-and-guarantees.md) rather than a bespoke merge engine, and another distribution would need a second composition engine. The manifest chooses the package set and configuration inside that system, not the system. Not planned.

## flake-pilot

Yes: the guest is whatever the registered image is, and building it is the user's job. `firecracker-pilot` takes a KIS image, which is [KIWI](https://osinside.github.io/kiwi/building_images/build_kis.html)'s kernel-initrd-system type — three components in one tarball — so the builder at this boundary is KIWI, run directly or through the Open Build Service, rather than any image toolchain: podman, mkosi, and koji do not emit that type. The one route open to a toolchain that is not KIWI is `pull --rootfs --kernel --initrd`, which takes three arbitrary files; the manual page documents it and nothing upstream demonstrates it.

The freedom is checkable rather than asserted, because upstream ships the description behind the VM this comparison registers. [`appstore/firecracker/claude/`](https://github.com/OSInside/flake-pilot/tree/920f41e/appstore/firecracker/claude) holds an `appliance.kiwi` that declares `<type image="kis" filesystem="ext2"/>` and lists the packages, a `config.sh` of build-time setup, and a `claude.sh` that builds the tarball in one command with no KIWI on the host:

```bash
podman run --privileged --rm -it \
    -v "$HOME/.kiwi_boxes:/root/.kiwi_boxes" \
    -v "$PWD:/claude.kiwi" -v "$PWD/image:/claude.kis" \
    public.ecr.aws/b9k1j9y6/kiwi:latest \
    system boxbuild --box tumbleweed -- \
    --description /claude.kiwi --target-dir /claude.kis
```

What step 4 asks for — what had to change on the host — is on the way back in. `flake-ctl firecracker pull` fetches over https and refuses any other transport unless `FLAKE_ALLOW_INSECURE_TRANSPORT` is set, and there is no firecracker counterpart to `flake-ctl podman load`, so a locally built tarball is served rather than opened:

```bash
(cd image && python3 -m http.server 8000) &
FLAKE_ALLOW_INSECURE_TRANSPORT=1 flake-ctl firecracker --user pull \
    --name myvm --kis-image http://localhost:8000/myvm.x86_64-1.0.0-0.tar.xz
```

The other cost is this comparison's reading rather than an upstream claim: the boundary is only as good as the image the registration names, and nothing in the registration attests to what is in it.

## glaipnir

No: `image/Containerfile` builds from `registry.opensuse.org/opensuse/tumbleweed:latest`, layered with a published per-agent image. `PACKAGES=(...)` and hooks extend that system; nothing selects a different one short of editing the `Containerfile`.

## podman

Yes: any image runs, so `/etc/os-release` inside is the user's choice. Under krun the kernel is libkrunfw's regardless of image — the userland is chosen, the kernel is not.

[^read]: Read at `vivarium` `ceb0027` and `glaipnir` `21ef389` on 2026-08-18; `podman` 5.x on 2026-08-19; `flake-pilot` `920f41e` on 2026-08-20.
