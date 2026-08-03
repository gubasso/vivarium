# 00 — Goals and non-goals

## What vivarium is

vivarium runs a project inside its own **microVM** — a separate guest kernel behind a hardware-virtualization boundary — described declaratively in Nix. It provides the "boring parts" of a secure sandbox: resolving a project's configuration, building the VM reproducibly, mounting the working directory, and wiring the network. Users describe a sandbox by composing reusable **images** and **config pieces** through a single **manifest**; the same manifest produces the same VM anywhere.

The command-line tool is a thin wrapper over a Nix build and a virtual machine runner. It does not reimplement isolation, kernels, or configuration merging — it orchestrates building blocks that already provide them.

## Goals

- **Strong isolation.** A separate guest kernel behind hardware virtualization, suitable for running untrusted code and autonomous agents. See [`../../decisions/ADR-0001-microvm-isolation-boundary.md`](../../decisions/ADR-0001-microvm-isolation-boundary.md).
- **Declarative.** The sandbox is described entirely in configuration, not assembled by imperative steps.
- **Deterministic.** A pinned lockfile makes a build reproducible across machines and over time. See [`04-composition-and-determinism.md`](./04-composition-and-determinism.md).
- **Composable.** Small, reusable images and pieces combine through manifests. See [`03-artifact-model.md`](./03-artifact-model.md).
- **Non-invasive.** A project's own development environment runs inside the sandbox untouched. See [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md).
- **Disposable.** A sandbox can be destroyed and rebuilt at any time with nothing of value lost. The work lives in the workspace — the user's own version-controlled directory on the host, which vivarium never writes to (N9) — and the environment lives in the manifest, which reproduces the same VM anywhere. Guest-side state is therefore **continuity, not a system of record**: persistent volumes exist so a warm restart is cheap, not so data is safe. `viv destroy` followed by `viv start` is the general recovery path, and its cost is a rebuild. See [`../../decisions/ADR-0080-the-sandbox-is-disposable.md`](../../decisions/ADR-0080-the-sandbox-is-disposable.md).
- **Cheap to run several.** Building on Nix is not only a determinism choice — it is what makes the marginal cost of an additional running project approach zero. Guests read the **host's store, shared read-only**: no per-VM store image is built, no store bytes are duplicated, and one host page cache serves every guest's store reads. This is a user-visible scaling property and therefore contract, not an implementation detail — and it is enforced as one: a per-VM store image is refused in every profile, because duplicating the base per sandbox would invert this goal rather than trade against it ([`../../decisions/ADR-0086-per-vm-store-duplication-is-refused.md`](../../decisions/ADR-0086-per-vm-store-duplication-is-refused.md)). Its cost is stated rather than hidden — a guest can enumerate the host's installed closure — which is one more reason N10 admits no exception. See [`06-workspace-and-project-environment.md`](./06-workspace-and-project-environment.md), [`17-resources-and-capacity.md`](./17-resources-and-capacity.md), and [`../../decisions/ADR-0038-guest-store-sharing.md`](../../decisions/ADR-0038-guest-store-sharing.md).
- **User-based.** All state lives under standard per-user directories; no privileged installation. See [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md).
- **Unobtrusive by default, restrictable on demand.** Open network by default, with an opt-in allowlist. See [`05-networking-and-egress.md`](./05-networking-and-egress.md).

## Non-goals

- **Not a shared-kernel container runtime.** The boundary is a VM, not namespaces; there is no shared-kernel mode.
- **Not a general orchestrator.** vivarium manages per-user, per-project sandboxes, not clusters, scheduling, or multi-tenant fleets.
- **Not a replacement for a project's dev environment.** It runs that environment; it does not define or absorb it.
- **Not a secrets manager.** It refuses to place secrets in the build and provides the runtime-injection channels — a forwarded authentication-agent channel and read-only credential mounts — but it does not store or rotate credentials, and it performs no cryptography on the user's behalf. Encrypted-at-rest secrets stay a legal and entirely user-owned shape: vivarium supplies no decryptor, executes no provider, and defines no secrets schema. The list of refusals that makes this enforceable rather than aspirational is in [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md).
- **Not a packaging or publishing tool** for the artifacts it builds.
- **Not a durability or data-protection layer for guest-side state.** This is the disposability goal read as a refusal, and it covers a class rather than a list: no VM snapshot or restore, no volume checkpoint or rewind, no backup integration, and no repair of a damaged guest. There is no suspend and no saved machine state — `stop` reaches **built** and nothing else ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)). Two independent reasons stand behind the refusal, and either alone would be enough. It would be **redundant**: a rebuild restores the image, generations restore the configuration ([`11-generations-and-build-history.md`](./11-generations-and-build-history.md)), and the workspace is the user's own version-controlled directory. And saved machine state is **secret-bearing**: a running guest holds runtime-injected secrets in memory ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)), so a memory image persists what N10 keeps out of the store, and vivarium would owe scrubbing or encryption machinery for a benefit it did not need. A user who wants a volume image copied has an ordinary file under the state root and their own tools ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)); vivarium supplies no verb for it.

## Audience

Developers and teams comfortable with a Nix-based workflow who want reproducible, strongly isolated sandboxes — particularly for running AI coding agents against real projects, where a deterministic sandbox yields deterministic runs.
