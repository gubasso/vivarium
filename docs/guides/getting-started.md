# Getting started

> **Design-intent walkthrough — not yet working.** This guide describes the *target* experience.
> None of these commands run today; nixvault is at the design stage. For what is actually
> implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md),
> which is the source of truth for status. Read this as the north star the implementation aims at.

This walkthrough follows a developer who wants to run a Rust project inside a nixvault sandbox. It
touches the command surface in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md)
and the artifacts in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).

## Prerequisites (intended)

- A host with hardware virtualization available (required by the isolation boundary; see
  [`../reference/spec/00-goals-and-non-goals.md`](../reference/spec/00-goals-and-non-goals.md)).
- A Nix toolchain on the host.
- A `rust-web` manifest in your config library that names a Rust image plus a few pieces (for
  example git, ssh-agent, direnv, and open egress). Manifest shape:
  [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).

## 1. Bind the project to a manifest

From the project directory:

```
$ cd ~/src/my-rust-api
$ nixvault init --manifest rust-web
```

This records the binding — a committed `.nixvault.toml` pointer or a user-registry entry — and
scaffolds a gitignored personal-override file. How the binding resolves later is specified in
[`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md).

## 2. Inspect what will be built

```
$ nixvault manifest show rust-web
$ nixvault show --resolved
```

`manifest show` prints the resolved image and ordered pieces; `show --resolved` renders the fully
merged configuration, so you can see the effective result of composition before booting. The merge
semantics are in
[`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md).

## 3. Boot the sandbox

```
$ nixvault up
```

This compiles the manifest to a flake, builds the VM with Nix, and boots it — mounting your current
directory read-write inside the guest. The working-directory path is supplied at this launch step,
not built in, which is why the same manifest is reproducible across machines
([`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md)).

## 4. Work inside the sandbox

```
$ nixvault exec -- cargo build
$ nixvault shell
```

`exec` runs a single command in the guest and returns its exit status; `shell` opens an interactive
session. Inside the workspace, your project's own development environment loads independently of
nixvault — the inner layer described in
[`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md).

By default the sandbox has open network access, so `cargo` can fetch crates with no setup. To restrict
egress, switch the manifest's egress mode to the allowlist; see
[`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md).

## 5. Stop

```
$ nixvault down
```

This stops the VM while preserving persistent volumes such as build caches, so the next `up` is fast.

## Where to go next

- Understand the overall shape: [`../explanation/architecture.md`](../explanation/architecture.md).
- Share the sandbox with a team without leaking secrets:
  [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).
