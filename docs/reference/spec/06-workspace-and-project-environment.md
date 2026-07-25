# 06 — Workspace and project environment

vivarium distinguishes two layers: the **sandbox** it manages, and the **project's own development
environment** that runs inside it. The separation and its rationale are in
[`../../decisions/ADR-0008-two-layer-separation.md`](../../decisions/ADR-0008-two-layer-separation.md).

## The mounted workspace

The tool mounts the user's working directory into the guest at `/workspaces/<repo>`, where `<repo>`
is the repository directory name, not the host absolute path. The mount is:

- **Read-write.** The project builds, editors write, and tools generate output in place. (Read-only
  mounts are reserved for injected identity such as credential directories; see
  [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md).)
- **A shared host directory**, exposed through the backend's filesystem sharing, so changes are
  visible on both sides.
- **Injected at launch time**, never built into the VM. The host path is supplied when the VM starts,
  which keeps the build pure and reproducible, per
  [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md).
- **Writable by the default guest user** used by `exec`/`shell`, as specified in
  [`12-exec-and-shell.md`](12-exec-and-shell.md).

Manifests and pieces may declare additional runtime mounts at other guest paths — extra
`/workspaces/<name>` repositories or mirrored host config files — through the mount schema decided
in
[`../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../decisions/ADR-0020-mount-and-config-mirroring-schema.md)
and
[`../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md):
a host `source` (host-side `${VAR}` expansion at launch), a guest `target` (`~` expands to the
guest home), and an optional `readonly` flag. The manifest declares them as TOML `[[mounts]]`
entries; a piece declares them as typed `vivarium.mounts` options
([`03-artifact-model.md`](03-artifact-model.md)); both compile into one merged list — declarations
concatenate across layers, and duplicate targets fail evaluation. These mounts follow the same
rule as the primary workspace: sources resolve through the launch channel
([`04-composition-and-determinism.md`](04-composition-and-determinism.md)), never as build inputs,
and shared layers reference the host only through portable variables. Config mirroring specifics
and the sharing rule live in [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md).

## The independent inner environment

A project may define its own development environment — a `flake.nix` with direnv, or an equivalent.
That environment is the **inner layer**: it lives in the repository, is owned by the project, and
must work identically whether or not the project runs inside a vivarium sandbox. vivarium never
modifies it.

Two Nix evaluations therefore exist and must not be conflated:

- The **outer** evaluation builds the sandbox (the VM) from the manifest, on the host, at build time.
- The **inner** evaluation builds the project's development environment, inside the guest, when a
  shell enters the workspace.

They are independent: separate configuration files, separate lockfiles, separate evaluations at
different times.

## What the guest must provide

For the inner environment to work, the sandbox base must ship a working Nix toolchain (with flakes
enabled) and direnv, so that entering the workspace loads the project's environment automatically.
Installing these in the guest is a requirement of the two-layer design, not an optional convenience.
`viv shell` enters the workspace as a login-interactive shell so direnv can load the inner
environment; details are in [`12-exec-and-shell.md`](12-exec-and-shell.md).

## Persistent volumes

The guest filesystem is three layers: the **immutable image** built from the store (rebuilt from
the manifest, writes lost), an **ephemeral runtime layer** whose writes vanish at shutdown, and the
**persistent volumes** — the project's mutable data, which survives `viv stop`, reboots, and
rebuilds. The model is decided in
[`../../decisions/ADR-0019-volume-model.md`](../../decisions/ADR-0019-volume-model.md):

- The **default volume** always exists, has the reserved name `default`, and is mounted at the
  guest user's home — so shell state, tool and language caches, dotfile state, and user-run data
  under `$HOME` persist without declaration.
- **Named volumes** are declared in the manifest (`[[volumes]]` with `name` and `mount`) or
  contributed by pieces (declarations concatenate; the same name with conflicting mountpoints fails
  evaluation). Each becomes its own disk, mounted at its declared guest path, with its own
  lifecycle under `viv volume`.
- **`[volume].persist`** lists extra guest paths (for example a system service's data directory)
  bind-mounted from inside the default volume, so their writes persist without a separate disk.
- Physically, each volume is one host-side disk image under the **state** root
  (`projects/<project-id>/<target>/volumes/<name>.img`) attached as a block device — state, never
  cache, because volume contents are user data and not regenerable
  ([`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md)). Identity is
  (project, target, name): manifests declare the shape, each bound project instantiates its own
  private volumes, and reattachment on `viv start` is automatic.
- **Anything written outside `$HOME`, a named volume's mountpoint, or a `persist` path is
  ephemeral** and lost at shutdown.
- Volumes are removed only on explicit request — `viv destroy` (all of them, unless
  `--keep-volumes`) or `viv volume rm` (which refuses while the VM runs) — never by `stop` or a
  rebuild (N18, [`08-invariants-and-guarantees.md`](08-invariants-and-guarantees.md)).

## The store inside the guest

The guest's Nix store may either be independent or share the host's store read-only for cache
reuse; this is an implementation trade-off between isolation and speed, made below the level of
this contract.
