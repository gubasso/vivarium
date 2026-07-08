# 06 — Workspace and project environment

nixvault distinguishes two layers: the **sandbox** it manages, and the **project's own development
environment** that runs inside it. The separation and its rationale are in
[`../../decisions/ADR-0008-two-layer-separation.md`](../../decisions/ADR-0008-two-layer-separation.md).

## The mounted workspace

The tool mounts the user's working directory into the guest at a fixed location so that the project's
files are present inside the VM. The mount is:

- **Read-write.** The project builds, editors write, and tools generate output in place. (Read-only
  mounts are reserved for injected identity such as credential directories; see
  [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md).)
- **A shared host directory**, exposed through the backend's filesystem sharing, so changes are
  visible on both sides.
- **Injected at launch time**, never built into the VM. The host path is supplied when the VM starts,
  which keeps the build pure and reproducible, per
  [`../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../decisions/ADR-0009-launch-time-workspace-path-injection.md).

## The independent inner environment

A project may define its own development environment — a `flake.nix` with direnv, or an equivalent.
That environment is the **inner layer**: it lives in the repository, is owned by the project, and
must work identically whether or not the project runs inside a nixvault sandbox. nixvault never
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

## Persistent volumes and the store

Build caches (for example a language package cache) may be mounted as persistent volumes so they
survive VM restarts, keeping rebuilds fast. The guest's Nix store may either be independent or share
the host's store read-only for cache reuse; this is an implementation trade-off between isolation and
speed, made below the level of this contract.
