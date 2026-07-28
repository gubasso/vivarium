# Getting started

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

This walkthrough follows a developer who wants to run a Rust project inside a vivarium sandbox. It touches the command surface in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) and the artifacts in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).

## Prerequisites (intended)

- A host with hardware virtualization available (required by the isolation boundary; see [`../reference/spec/00-goals-and-non-goals.md`](../reference/spec/00-goals-and-non-goals.md)).
- A Nix toolchain on the host.
- A `rust-web` manifest in your config library that names a Rust image plus a few pieces (for example git, ssh-agent, direnv, and open egress). Manifest shape: [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).

## 1. Bind the project to a manifest

From the project directory:

```console
$ cd ~/src/my-rust-api
$ viv init --manifest rust-web         # preview the registry entry it would write
$ viv init --manifest rust-web --write # record the binding
```

`viv init` records the binding in your per-user project registry (state); it writes nothing into the project's own tree or into your config. Run it without `--write` to preview the exact entry first. How the binding resolves later is specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md).

## 2. Inspect what will be built

```console
$ viv manifest show rust-web
$ viv config eval
```

`manifest show` prints the resolved image and ordered pieces; `config eval` renders the fully merged configuration, so you can see the effective result of composition before booting (and `config sources` shows which layer each value came from). The merge semantics are in [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md).

## 3. Boot the sandbox

```console
$ viv start
```

This compiles the manifest to a flake, builds the VM with Nix, and boots it — mounting your current project read-write at `/workspaces/<repo>` inside the guest. The working-directory path is supplied at this launch step, not built in, which is why the same manifest is reproducible across machines ([`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md)).

`start` boots the sandbox in the background and returns; running it again on an unchanged project is a no-op. Each successful build is kept as a numbered generation you can list with `viv generations list` and boot with `viv start --generation <n>` — see [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md) and [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md).

## 4. Work inside the sandbox

```console
$ viv exec -- cargo build
$ viv exec -t -- cargo test
$ viv shell
```

`exec` runs one command with transparent stdio and returns the guest status. It defaults to no PTY, so use `-t` for interactive terminal behavior.

`shell` opens a login-interactive PTY shell. Multiple `exec`/`shell` sessions for the same project share the same VM and see the repository at `/workspaces/<repo>`.

Inside the workspace, your project's own development environment loads independently of vivarium — the inner layer described in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md). Detailed `exec`/`shell` behavior is specified in [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md).

By default the sandbox has open network access, so `cargo` can fetch crates with no setup. To restrict egress, switch the manifest's egress mode to the allowlist; see [`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md).

## 5. Stop

```console
$ viv stop
```

This gracefully stops the VM while preserving persistent volumes — your home directory in the guest, with its caches and tool state — so the next `start` is fast and warm. To erase the project instead (build history, volumes, runtime state), use `viv destroy`; see [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md).

## Where to go next

- Understand the overall shape: [`../explanation/architecture.md`](../explanation/architecture.md).
- Share the sandbox with a team without leaking secrets: [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).
