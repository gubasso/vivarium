# Disk model compared with containers

This page explains the conceptual difference between vivarium's storage model and container layers. For implemented behavior, consult [implementation status](../reference/implementation-status.md).

A conventional container image packages filesystem layers and gives each running container a writable layer. Reusing an image avoids some transfer, but each system still reasons in terms of image layers and per-container changes.

vivarium starts from Nix store paths, and that changes the unit of sharing. Nix shares the individual store path rather than a position in a layer stack, so two sandboxes that agree on one dependency share exactly that dependency, whatever else they disagree about — where two container images sharing a base but diverging one layer early share nothing below the divergence. Immutable build outputs are already content-addressed and shared by the host, so the accepted design exposes the host store read-only rather than copying it into every VM, and no per-VM store image is materialized at start or reclaimed at teardown. Guest-local writes form a separate upper store whose database and data persist together. Project files remain ordinary user-owned host files shared at a stable guest path, while declared volumes provide explicit guest durability.

The consequence is the one [ADR-0086](../decisions/ADR-0086-per-vm-store-duplication-is-refused.md) rests on: the marginal disk cost of an additional sandbox is what that sandbox uniquely needs, which is why a per-VM store copy is refused permanently rather than merely deferred.

This yields three different lifetimes instead of one container-layer abstraction:

- immutable build closure paths shared from the host;
- project-authored files owned by the user outside the VM;
- guest-local mutable volumes and the guest store upper layer.

The exact storage topology and collection boundaries live in [guest store and volumes](./guest-store-and-volumes.md). The sharing boundary, identity translation, cache policy, and daemon budgets live in [shared filesystems](./shared-filesystems.md). Exact normative behavior remains in the [workspace and project environment specification](../reference/spec/06-workspace-and-project-environment.md) and [resource specification](../reference/spec/17-resources-and-capacity.md).
