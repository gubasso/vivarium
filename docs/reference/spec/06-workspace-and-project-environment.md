# 06 — Workspace and project environment

vivarium distinguishes two layers: the **sandbox** it manages, and the **project's own development environment** that runs inside it. The separation and its rationale are in [`../../decisions/ADR-0008-two-layer-separation.md`](../../decisions/ADR-0008-two-layer-separation.md).

## The mounted workspace

The tool mounts the user's working directory into the guest at `/workspaces/<repo>`, where `<repo>` is the repository directory name, not the host absolute path. The mount is:

- **Read-write.** The project builds, editors write, and tools generate output in place. (Read-only mounts are reserved for injected identity such as credential directories; see [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md).)
- **A shared host directory**, exposed through the backend's filesystem sharing, so changes are visible on both sides.
- **Injected at launch time**, never built into the VM. The host path is supplied when the VM starts, which keeps the build pure and reproducible, per [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md).
- **Writable by the default guest user** used by `exec`/`shell`, as specified in [`12-exec-and-shell.md`](./12-exec-and-shell.md).

Manifests and pieces may declare additional runtime mounts at other guest paths — extra `/workspaces/<name>` repositories or mirrored host config files — through the mount schema decided in [`../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../decisions/ADR-0020-mount-and-config-mirroring-schema.md) and [`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md): a host `source` (host-side `${VAR}` expansion at launch), a guest `target` (`~` expands to the guest home), and an optional `readonly` flag. The manifest declares them as TOML `[[mounts]]` entries; a piece declares them as typed `vivarium.mounts` options ([`03-artifact-model.md`](./03-artifact-model.md)); both compile into one merged list — declarations concatenate across layers, and duplicate targets fail evaluation. These mounts follow the same rule as the primary workspace: sources resolve through the launch channel ([`04-composition-and-determinism.md`](./04-composition-and-determinism.md)), never as build inputs, and shared layers reference the host only through portable variables. Config mirroring specifics and the sharing rule live in [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md).

### How shares are served and confined

A project may declare **multiple writable paths** (extra `/workspaces/<name>` repositories) and **multiple additional bind mounts**, each chosen read-only or read-write through the mount schema's `readonly` flag. Every declared share — the primary workspace and each extra mount — is served by its **own unprivileged virtiofsd process**, confined under the N20 launch profile ([`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md), [`../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md)): `--sandbox=namespace`, seccomp on, with only that share's host path in its view, so one share can never reach another or the wider host filesystem.

Read-only is enforced on both sides: the host source is exposed read-only **and** the guest mount is `ro,nodev,nosuid,noexec`, so a read-only mount can carry data but never executables or device nodes, and writes are confined to exactly the declared read-write paths. Guest scratch — `/tmp` and other non-persistent locations — is the per-VM ephemeral layer above, never a host mount.

### Cache policy per share

A share's **cache mode governs coherency, not confinement** — it decides how long the guest may trust its own view of a share, and it is deliberately no part of the N20 profile ([`../../decisions/ADR-0039-share-cache-policy.md`](../../decisions/ADR-0039-share-cache-policy.md)). Each share takes the policy its content justifies:

| Share                                          | Cache policy                 | Why                                                                                                                        |
| ---------------------------------------------- | ---------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| The read-only guest store share (below)        | cache aggressively           | Content-addressed and immutable, so there is no stale view to guard against — a proof, not a trade-off                     |
| The primary workspace and any read-write mount | the daemon's bounded default | Host-side edits become visible in the guest within the timeout, which is what "edit on the host, build in the guest" needs |
| Read-only mirrored config                      | the daemon's bounded default | Changes rarely; the bounded default is more than sufficient                                                                |

Caching is never disabled outright on the workspace. This workload is dominated by directory walks and small-file metadata, and forbidding client caching turns every path lookup into a round trip.

**Keep regenerable caches off the share.** Language and package-manager caches, dependency trees, and build output directories belong on a persistent volume rather than under the workspace share: a volume is a local block device to the guest, while a share pays a round trip per file. Where a toolchain insists on writing them inside the repository, `[volume].persist` (below) is the mechanism.

## The independent inner environment

A project may define its own development environment — a `flake.nix` with direnv, or an equivalent. That environment is the **inner layer**: it lives in the repository, is owned by the project, and must work identically whether or not the project runs inside a vivarium sandbox. vivarium never modifies it. The sole thing vivarium writes into the project tree is its own self-ignored `.vivarium/` identity marker (N9, N21), which is inert to this inner layer; see [`15-project-identity.md`](./15-project-identity.md).

Two Nix evaluations therefore exist and must not be conflated:

- The **outer** evaluation builds the sandbox (the VM) from the manifest, on the host, at build time.
- The **inner** evaluation builds the project's development environment, inside the guest, when a shell enters the workspace.

They are independent: separate configuration files, separate lockfiles, separate evaluations at different times.

## What the guest must provide

For the inner environment to work, the sandbox base must ship a working Nix toolchain (with flakes enabled) and direnv, so that entering the workspace loads the project's environment automatically. Installing these in the guest is a requirement of the two-layer design, not an optional convenience. `viv shell` enters the workspace as a login-interactive shell so direnv can load the inner environment; details are in [`12-exec-and-shell.md`](./12-exec-and-shell.md).

## Persistent volumes

The guest filesystem is three layers: the **immutable image** built from the store (rebuilt from the manifest, writes lost), an **ephemeral runtime layer** whose writes vanish at shutdown, and the **persistent volumes** — the project's mutable data, which survives `viv stop`, reboots, and rebuilds. The model is decided in [`../../decisions/ADR-0019-volume-model.md`](../../decisions/ADR-0019-volume-model.md):

- The **default volume** always exists, has the reserved name `default`, and is mounted at the guest user's home — so shell state, tool and language caches, dotfile state, and user-run data under `$HOME` persist without declaration.
- **Named volumes** are declared in the manifest (`[[volumes]]` with `name` and `mount`) or contributed by pieces through the `vivarium.volumes` option (declarations concatenate; the same name with conflicting mountpoints fails evaluation). Each becomes its own disk, mounted at its declared guest path, with its own lifecycle under `viv volume`. Unlike mounts and resource ceilings, a volume declaration is **build-channel** — the guest mountpoint is guest system configuration, so adding a volume rebuilds, and only the host image path and virtual size resolve at launch ([`../../decisions/ADR-0041-resource-and-volume-channel-classification.md`](../../decisions/ADR-0041-resource-and-volume-channel-classification.md)).
- **`[volume].persist`** lists extra guest paths (for example a system service's data directory) bind-mounted from inside the default volume, so their writes persist without a separate disk.
- Physically, each volume is one host-side disk image under the **state** root (`projects/<project-id>/<target>/volumes/<name>.img`) attached as a block device — state, never cache, because volume contents are user data and not regenerable ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)). Identity is (project, target, name): manifests declare the shape, each bound project instantiates its own private volumes, and reattachment on `viv start` is automatic. The `<project-id>` and `<target>` components are defined in [`15-project-identity.md`](./15-project-identity.md).
- Each image is **sparse and raw**, created lazily on the first `viv start` that needs it, and its declared size is a **virtual ceiling**: the image occupies what its contents occupy, and space freed inside the guest is returned to the host by periodic and on-demand trim (N22, [`../../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../../decisions/ADR-0037-volume-disk-format-and-reclamation.md)). A volume's ceiling may be raised between boots; it is never lowered in place. Sizes, defaults, and the allocated-against-virtual reporting are in [`17-resources-and-capacity.md`](./17-resources-and-capacity.md).
- **Anything written outside `$HOME`, a named volume's mountpoint, or a `persist` path is ephemeral** and lost at shutdown.
- Volumes are removed only on explicit request — `viv destroy` (all of them, unless `--keep-volumes`) or `viv volume rm` (which refuses while the VM runs) — never by `stop` or a rebuild (N18, [`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)).
- An **orphan** is a volume image still on disk under the project's state that no current layer declares — the residue of a volume dropped from the manifest or from a piece. It keeps occupying space and is never removed implicitly, precisely because a declaration can be removed by accident and the data is not regenerable. `viv volume list` flags orphans so the space is visible before `viv volume rm` reclaims it.

Exit codes for `viv volume list` / `rm` / `trim` follow the per-command matrix in [`14-exit-codes.md`](./14-exit-codes.md) — notably `75` when `rm` refuses because the VM is still running (stop first).

## The store inside the guest

The guest reads the **host's Nix store, shared read-only**, decided in [`../../decisions/ADR-0038-guest-store-sharing.md`](../../decisions/ADR-0038-guest-store-sharing.md). This is contract, not an implementation detail, because it is what makes each additional running project nearly free: no per-VM store image is built, no store bytes are duplicated, and one host page cache serves every guest.

- The store share is read-only on both sides and served by its own confined daemon, exactly like every other share, with the aggressive cache policy justified above.
- A **writable overlay** sits above it inside the guest, so `nix build` and the project's own inner environment work normally ([`04-composition-and-determinism.md`](./04-composition-and-determinism.md)); writes land in the overlay, never in the host store.
- The trade is stated rather than hidden: a guest can enumerate every path in the host store. The store is world-readable by construction and never holds secrets (N10, [`../../decisions/ADR-0010-secrets-never-in-nix-store.md`](../../decisions/ADR-0010-secrets-never-in-nix-store.md)), and the boundary protects against escape, not against a guest learning which packages the host has built.
- An independent per-VM store remains admissible for a future hardened profile; it is not the default, and choosing it costs a store image per VM per generation.
