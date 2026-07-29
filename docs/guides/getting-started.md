# Getting started

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

This walkthrough follows a developer who wants to run a Rust project inside a vivarium sandbox. It touches the command surface in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) and the artifacts in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).

## Prerequisites (intended)

- A host with hardware virtualization available (required by the isolation boundary; see [`../reference/spec/00-goals-and-non-goals.md`](../reference/spec/00-goals-and-non-goals.md)).
- A Nix toolchain on the host.
- A `rust-web` manifest in your config library that names a Rust image plus a few pieces (for example git, ssh-agent, direnv, and open egress). Manifest shape: [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md); how each bare name resolves to a file is decided in [the library-layout ADR](../decisions/ADR-0045-config-root-library-layout-and-name-resolution.md).

## 1. Bind the project to a manifest

From the project directory:

```console
$ cd ~/src/my-rust-api
$ viv init --manifest rust-web         # preview the registry entry it would write
$ viv init --manifest rust-web --write # record the binding
```

`viv init` records the binding in your per-user project registry (state); it writes nothing into the project's own tree or into your config. Run it without `--write` to preview the exact entry first. How the binding resolves later is specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md), and [the decision that made your config read-only to the tool](../decisions/ADR-0011-config-read-only-binding-in-state.md) explains why there is no file to commit or gitignore.

## 2. Inspect what will be built

```console
$ viv manifest show rust-web
$ viv config eval
```

`manifest show` prints the resolved image and ordered pieces; `config eval` renders the fully merged configuration, so you can see the effective result of composition before booting (and `config sources` shows which layer each value came from). The merge semantics are in [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md); [the decision gathering these views into one inspection namespace](../decisions/ADR-0022-config-inspection-namespace.md) explains why they are separate commands rather than flags on one.

## 3. Boot the sandbox

```console
$ viv start
```

This compiles the manifest to a flake ([the decision to make TOML compile rather than be interpreted](../decisions/ADR-0004-toml-manifest-compiles-to-flake.md)), builds the VM with Nix, and boots it — mounting your current project read-write at `/workspaces/<repo>` inside the guest. The working-directory path is supplied at this launch step, not built in, which is why the same manifest is reproducible across machines ([`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md), decided in [the workspace-mount ADR](../decisions/ADR-0017-workspace-mount-path-and-extra-mounts.md)).

`start` boots the sandbox in the background and returns; running it again on an unchanged project is a no-op, because [the lifecycle decision](../decisions/ADR-0013-vm-lifecycle-and-up.md) made starting detached, idempotent, and non-destructive. Each successful build is kept as a numbered generation you can list with `viv generations list` and boot with `viv start --generation <n>` — see [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md) and [`../reference/spec/11-generations-and-build-history.md`](../reference/spec/11-generations-and-build-history.md), and [the decision pinning each generation as a garbage-collector root](../decisions/ADR-0014-build-generations-and-gc-roots.md) for why an old build survives until you prune it.

## 4. Work inside the sandbox

```console
$ viv exec -- cargo build
$ viv exec -t -- cargo test
$ viv shell
```

`exec` runs one command with transparent stdio and returns the guest status. It defaults to no PTY, so use `-t` for interactive terminal behavior.

`shell` opens a login-interactive PTY shell. Multiple `exec`/`shell` sessions for the same project share the same VM and see the repository at `/workspaces/<repo>`.

Inside the workspace, your project's own development environment loads independently of vivarium — the inner layer described in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md). Detailed `exec`/`shell` behavior is specified in [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md), and [the decision fixing the control transport and exec contract](../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) explains where the status boundary falls.

By default the sandbox has open network access, so `cargo` can fetch crates with no setup. That default is deliberate, and [the decision behind it](../decisions/ADR-0007-default-open-egress.md) frames open egress as an exfiltration risk rather than a weaker isolation boundary. To restrict egress, switch the manifest's egress mode to the allowlist; see [`../reference/spec/05-networking-and-egress.md`](../reference/spec/05-networking-and-egress.md).

## 5. Run several projects at once

Nothing about the above is one-project-at-a-time. Repeat steps 1 and 3 in another repository and you have a second sandbox; each has its own kernel, its own home volume, and as many attached terminals as you care to open. `viv status -g` shows them all:

```console
$ viv status -g
PROJECT          STATE     MEM (used/ceiling)   VCPU  DISK (alloc/virtual)  SESS  UP
my-rust-api      running    2.1 GiB / 8 GiB      8     4.2 GiB / 32 GiB      3    3h12m
web-frontend     running    5.8 GiB / 8 GiB      8    11.7 GiB / 32 GiB      1    1h04m
data-pipeline    running    1.4 GiB / 8 GiB      8     2.9 GiB / 32 GiB      2      22m

host: 9.6 GiB available of 31.2 GiB - memory pressure (60s): 0.4%
```

Read that table once and the resource model explains itself: **the ceiling is what a project may use, the used column is what it actually costs.** Three projects declaring 8 GiB each are not holding 24 GiB — [the decision making declared resources ceilings rather than reservations](../decisions/ADR-0035-elastic-guest-memory-model.md) is what buys that. You never set those numbers — vivarium derives them from the host — and if you start a fourth project when memory is genuinely tight, `viv start` says so and lets you decide rather than deciding for you, per [the admission-control decision](../decisions/ADR-0036-host-resource-scoping-and-admission-control.md). The full model is in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).

If a long-running project has accumulated cached memory you want back, `viv trim` reclaims it on the spot. Nothing reclaims automatically.

## 6. Stop

```console
$ viv stop
```

This gracefully stops the VM while preserving persistent volumes — your home directory in the guest, with its caches and tool state — so the next `start` is fast and warm. `viv stop --all` does the same for every running project at once. To erase the project instead (build history, volumes, runtime state), use `viv destroy`; see [`../reference/spec/10-vm-lifecycle.md`](../reference/spec/10-vm-lifecycle.md). That `stop` removes nothing and `destroy` is the only destructive verb is [a deliberate teardown boundary](../decisions/ADR-0018-lifecycle-verbs-and-teardown-boundary.md).

## Where to go next

- Understand the overall shape: [`../explanation/architecture.md`](../explanation/architecture.md).
- Share the sandbox with a team without leaking secrets: [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md).
