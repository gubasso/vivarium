# Why several sandboxes cost so little disk

The mental model for vivarium's disk economics, explained against the thing most readers already know: a container image. This page teaches the shape and the comparison; the contract it rests on is [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) ("The store inside the guest"), and the reasoning is in [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md).

## The container model, stated fairly

A container image built from a `Containerfile` is a stack of layers, each a tarball of filesystem changes, addressed by digest ([OCI Image Format Specification](https://github.com/opencontainers/image-spec/blob/main/layer.md)). Two containers started from one image share every layer: the image is stored once, and each container adds only a thin writable layer for what it changes. Remove the container and that layer goes with it. Start a second project from the same image and you pay nothing extra for the base.

This is a good design, and running several projects under it is genuinely cheap. Tools like Podman extend it to microVM isolation — [`crun-vm`](https://github.com/containers/crun-vm) and [libkrun](https://github.com/containers/libkrun) run an OCI payload behind a hardware boundary while keeping the image model underneath — so "microVM isolation" and "container-style image sharing" are not opposed, and the comparison below is not isolation against economy.

## Where vivarium differs, and why it is not merely a different spelling

The comparison is worth making because the two models share the goal and differ in the unit of sharing.

**Layers versus store paths.** An OCI layer is a position in a stack, so sharing is prefix-shaped: two images that install the same compiler share it only if they share every layer up to and including the one that installed it. Reorder two instructions in one `Containerfile` and the compiler is stored twice. Nix's unit is the individual store path, named by a hash of everything that produced it, so two unrelated images that happen to contain the same compiler contain the same path and share exactly one copy of it on disk. Nothing has to be ordered correctly for that to happen, and nobody has to maintain the discipline that keeps it happening.

**No per-VM image is materialized at all.** In the container model, the runtime writes a per-container layer to disk. vivarium's guests read the host's `/nix/store` directly over the read-only share, so a running sandbox adds no image of its own — there is no per-VM store to build at `viv start` and none to reclaim at teardown. What a guest writes lands in a guest-local overlay above that share, exactly as the spec describes; the host store is never modified from inside a sandbox.

The practical effect is the one the [`../../README.md`](../../README.md) highlight claims: the fifth concurrent project costs close to nothing on disk.

## What is not shared, and never will be

Two limits belong to the mental model rather than to the fine print.

**Memory is not deduplicated.** The saving above is a saving on the host's disk and its page cache. Guest memory is per-guest, and no design choice can change that — the constraint and its cause are stated in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md). Five near-identical sandboxes each pay for what they read. This is the honest asymmetry against containers, which share one kernel and therefore one page cache all the way up.

**Presence is not usability.** A host store path being visible through the share does not make it usable by the guest's Nix, because validity lives in a database the guest owns. Today that database knows the guest's own closure and nothing more, so the disk saving is proven for the base image and does not yet extend to the project's own dependencies. The measured limit and the policy for lifting it are in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and [`../decisions/ADR-0083-inner-layer-store-registration-is-on-demand.md`](../decisions/ADR-0083-inner-layer-store-registration-is-on-demand.md). It is worth separating from the disk question it is easily confused with: the bytes are already shared; what is open is whether the guest may use them.

## Reclamation has a different shape, and this surprises people

`podman rm` frees a container's writable layer immediately, because that layer belongs to that container and to nothing else. vivarium cannot work that way, and the reason is the same one that makes the base nearly free: the bytes are in a store that other sandboxes, other generations, and the host itself may still depend on.

So teardown and reclamation are two verbs rather than one. `viv destroy` ends a project's sandbox and drops what pinned its builds; the sweep that actually reclaims store contents is separate and machine-wide, because only a machine-wide view can tell what is still reachable. [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md) and [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md) own the two halves. "Recoverable" is true; "recovered the moment the box is gone" is not.

## The design this rules out

Because the sharing is so cheap, the tempting next step is to share a store that guests may _write_ — one pool, every sandbox contributing to it, so a package built in one project is free in the next. Container tooling does exactly this, successfully.

It is refused here, permanently, and the reason is instructive: it works for containers only because they share a kernel. Two vivarium guests do not, and a Nix store's database cannot span that gap — nor can the file locking that would keep it honest. The full argument, with sources, is in [`../decisions/ADR-0038-guest-store-sharing.md`](../decisions/ADR-0038-guest-store-sharing.md). It is a clean illustration of the general shape of this project's trades: the isolation boundary is not free, and the things it costs are usually the things that quietly assumed one kernel.
