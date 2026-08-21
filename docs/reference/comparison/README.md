# vivarium and the alternatives

Four tools that put a coding agent somewhere it cannot wreck your laptop. A capability name links to the scenario that decides it; a verdict links to what that tool did when the scenario was run against it. Row derivation: [`feature-sweep.md`](./feature-sweep.md). Provenance: [`sources.md`](./sources.md). The two jobs that span rows, end to end: [`walkthroughs.md`](./walkthroughs.md).

## Methodology

Every capability row is read at one isolation class for every tool: a microVM behind a hardware-virtualization boundary with its own guest kernel, the only class vivarium has. The engine inside that class is not held fixed, because the subjects do not agree on one. Holding the class fixed and letting the engine vary is the claim these tables can defend, and it is the one they already implement — `firecracker` for one column, libkrun for three.

flake-pilot is the subject that forces the distinction, because it reaches the class by two routes and upstream publishes a `claude` registration for each. Both are read here, and neither stands in for the other:

| subject                   | read at                                                                          |
| ------------------------- | -------------------------------------------------------------------------------- |
| `vivarium`                | its one microVM                                                                  |
| `flake-pilot` firecracker | `firecracker-pilot`, at the upstream `claude` firecracker registration           |
| `flake-pilot` krun        | `podman-pilot` at `--runtime krun`, at the upstream `claude` `krun` registration |
| `glaipnir`                | the libkrun microVM                                                              |
| `podman`                  | `podman run --runtime krun`                                                      |

Reading flake-pilot at one of those routes and calling it the microVM would have been a selection the phrase does not make. [The engine row](./scenarios/engine-choice.md#flake-pilot) records that the route is the user's, fixed at registration, and that upstream states the two are not equivalent. So the two routes are two columns, and a flake-pilot verdict always says which route it is a verdict about.

Every page in this directory carries the revision and date it was read at in a footnote at the foot of the file, so a verdict opens on the verdict rather than on its provenance.

One row asks one thing. Where a cell would have to hedge because two capabilities sat under one label, the row is split until each verdict is a single claim, and the two halves link to each other. That is why several rows come in pairs, and why `⚠️ partial` is rare: a hedge here means the answer really is in between, not that the question was two questions.

Legend: ✅ yes · ⚠️ partial · ❌ no · ➖ n/a

- `*` — specified, not built yet
- `†` — deliberately not planned; the cell links to what forecloses it
- `‡` — the mechanism is the tool's and the arrangement is yours; it works, and it works each time because you invoked it

## Isolation backends

What each tool can run the workload on, and how the backend gets chosen. This is the only cross-backend section.

| backend question                                                                    | `vivarium`                                                          | `flake-pilot` firecracker                                         | `flake-pilot` krun                                                | `glaipnir`                                          | `podman`                                                |
| ----------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------- |
| [MicroVM, own guest kernel](./scenarios/own-kernel.md)                              | ✅ only mode                                                        | [✅ firecracker](./scenarios/own-kernel.md#flake-pilot)           | [✅ libkrun](./scenarios/own-kernel.md#flake-pilot)               | [✅ libkrun](./scenarios/own-kernel.md#glaipnir)    | [✅ `--runtime krun`](./scenarios/own-kernel.md#podman) |
| [OCI container, shared kernel](./scenarios/shared-kernel.md)                        | [❌ no†](./scenarios/shared-kernel.md#vivarium)                     | [✅ `crun`](./scenarios/shared-kernel.md#flake-pilot)             | [✅ `crun`](./scenarios/shared-kernel.md#flake-pilot)             | [✅ default](./scenarios/shared-kernel.md#glaipnir) | ✅ default                                              |
| [Who selects the backend](./scenarios/engine-choice.md)                             | [nobody — capability class†](./scenarios/engine-choice.md#vivarium) | [user, at registration](./scenarios/engine-choice.md#flake-pilot) | [user, at registration](./scenarios/engine-choice.md#flake-pilot) | [host probe](./scenarios/engine-choice.md#glaipnir) | user, per call                                          |
| [Host without KVM](./scenarios/no-kvm.md)                                           | [refuses](./scenarios/no-kvm.md#vivarium)                           | [container backends only](./scenarios/no-kvm.md#flake-pilot)      | [container backends only](./scenarios/no-kvm.md#flake-pilot)      | [falls back, warns](./scenarios/no-kvm.md#glaipnir) | [container runtimes only](./scenarios/no-kvm.md#podman) |
| [No flag selects a weaker boundary](./scenarios/boundary-flag.md)                   | [✅ yes](./scenarios/boundary-flag.md#vivarium)                     | [✅ yes](./scenarios/boundary-flag.md#flake-pilot)                | [✅ yes](./scenarios/boundary-flag.md#flake-pilot)                | [❌ no](./scenarios/boundary-flag.md#glaipnir)      | [❌ no](./scenarios/boundary-flag.md#podman)            |
| [No file you did not write selects a weaker boundary](./scenarios/boundary-file.md) | [✅ yes](./scenarios/boundary-file.md#vivarium)                     | [❌ no](./scenarios/boundary-file.md#flake-pilot)                 | [❌ no](./scenarios/boundary-file.md#flake-pilot)                 | [✅ yes](./scenarios/boundary-file.md#glaipnir)     | [❌ no](./scenarios/boundary-file.md#podman)            |
| [macOS](./scenarios/macos.md)                                                       | [❌ no](./scenarios/macos.md#vivarium)                              | ❌ no                                                             | ❌ no                                                             | [container only](./scenarios/macos.md#glaipnir)     | [container only](./scenarios/macos.md#podman)           |

## Data isolation

What crosses the boundary, and what the tool can see once it is inside.

| capability                                                                                                 | `vivarium`                                             | `flake-pilot` firecracker                                | `flake-pilot` krun                                       | `glaipnir`                                             | `podman`                                              |
| ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ | -------------------------------------------------------- | -------------------------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------- |
| [The tool derives the workspace mount, with no host path named](./scenarios/workspace-mount.md)            | [✅ yes](./scenarios/workspace-mount.md#vivarium)      | [❌ no](./scenarios/workspace-mount.md#flake-pilot)      | [❌ no](./scenarios/workspace-mount.md#flake-pilot)      | [✅ yes](./scenarios/workspace-mount.md#glaipnir)      | [❌ no](./scenarios/workspace-mount.md#podman)        |
| [Work stays at its host path](./scenarios/host-path.md)                                                    | [✅ yes](./scenarios/host-path.md#vivarium)            | [➖ n/a](./scenarios/host-path.md#flake-pilot)           | [✅ yes‡](./scenarios/host-path.md#flake-pilot)          | [❌ no](./scenarios/host-path.md#glaipnir)             | [✅ yes‡](./scenarios/host-path.md#podman)            |
| [Choose which host paths cross, in a file rather than on the command line](./scenarios/choosing-mounts.md) | [✅ yes](./scenarios/choosing-mounts.md#vivarium)      | [➖ n/a](./scenarios/choosing-mounts.md#flake-pilot)     | [✅ yes‡](./scenarios/choosing-mounts.md#flake-pilot)    | [❌ no](./scenarios/choosing-mounts.md#glaipnir)       | [✅ yes‡](./scenarios/choosing-mounts.md#podman)      |
| [The caller chooses which host environment variables cross](./scenarios/environment.md)                    | [✅ yes](./scenarios/environment.md#vivarium)          | [✅ yes](./scenarios/environment.md#flake-pilot)         | [✅ yes](./scenarios/environment.md#flake-pilot)         | [❌ no](./scenarios/environment.md#glaipnir)           | ✅ yes                                                |
| [Refuses a mount that would expose the host session](./scenarios/session-sockets.md)                       | [✅ yes](./scenarios/session-sockets.md#vivarium)      | [➖ n/a](./scenarios/session-sockets.md#flake-pilot)     | [❌ no](./scenarios/session-sockets.md#flake-pilot)      | [❌ no](./scenarios/session-sockets.md#glaipnir)       | [❌ no](./scenarios/session-sockets.md#podman)        |
| [Use an SSH or GPG key without the key entering the sandbox](./scenarios/key-material.md)                  | [✅ yes](./scenarios/key-material.md#vivarium)         | [❌ no](./scenarios/key-material.md#flake-pilot)         | [❌ no](./scenarios/key-material.md#flake-pilot)         | [❌ no](./scenarios/key-material.md#glaipnir)          | [❌ no](./scenarios/key-material.md#podman)           |
| [Secrets are kept out of the built artifact](./scenarios/secrets-in-the-build.md)                          | [✅ yes](./scenarios/secrets-in-the-build.md#vivarium) | [❌ no](./scenarios/secrets-in-the-build.md#flake-pilot) | [❌ no](./scenarios/secrets-in-the-build.md#flake-pilot) | [✅ yes](./scenarios/secrets-in-the-build.md#glaipnir) | [✅ yes‡](./scenarios/secrets-in-the-build.md#podman) |
| [Keeping secrets out of the build is enforced](./scenarios/secrets-enforced.md)                            | [⚠️ partial](./scenarios/secrets-enforced.md#vivarium)  | [❌ no](./scenarios/secrets-enforced.md#flake-pilot)     | [❌ no](./scenarios/secrets-enforced.md#flake-pilot)     | [❌ no](./scenarios/secrets-enforced.md#glaipnir)      | [❌ no](./scenarios/secrets-enforced.md#podman)       |
| [Commit an encrypted secret alongside the config](./scenarios/shipping-a-secret.md)                        | [✅ yes‡](./scenarios/shipping-a-secret.md#vivarium)   | [❌ no](./scenarios/shipping-a-secret.md#flake-pilot)    | [❌ no](./scenarios/shipping-a-secret.md#flake-pilot)    | [❌ no](./scenarios/shipping-a-secret.md#glaipnir)     | [✅ yes‡](./scenarios/shipping-a-secret.md#podman)    |
| [Scopes credentials per app out of the box](./scenarios/per-tool-credentials.md)                           | [❌ no†](./scenarios/per-tool-credentials.md#vivarium) | [❌ no](./scenarios/per-tool-credentials.md#flake-pilot) | [❌ no](./scenarios/per-tool-credentials.md#flake-pilot) | [✅ yes](./scenarios/per-tool-credentials.md#glaipnir) | [❌ no](./scenarios/per-tool-credentials.md#podman)   |

## Network isolation

Where the tool can reach, and who decides. Reachable, not default: vivarium's egress is unrestricted until a manifest says otherwise, and the first row asks whether the deny posture exists at all.

| capability                                                          | `vivarium`                                            | `flake-pilot` firecracker                                | `flake-pilot` krun                                        | `glaipnir`                                      | `podman`                                            |
| ------------------------------------------------------------------- | ----------------------------------------------------- | -------------------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------- | --------------------------------------------------- |
| [Egress can be default-deny](./scenarios/default-deny-egress.md)    | [✅ yes](./scenarios/default-deny-egress.md#vivarium) | [✅ yes](./scenarios/default-deny-egress.md#flake-pilot) | [✅ yes‡](./scenarios/default-deny-egress.md#flake-pilot) | ❌ no                                           | [✅ yes](./scenarios/default-deny-egress.md#podman) |
| [Allowlist by destination name](./scenarios/allowlist.md)           | [✅ yes](./scenarios/allowlist.md#vivarium)           | ❌ no                                                    | ❌ no                                                     | ❌ no                                           | ❌ no                                               |
| [Stays off a corporate VPN](./scenarios/corporate-vpn.md)           | [❌ no](./scenarios/corporate-vpn.md#vivarium)        | [⚠️ partial](./scenarios/corporate-vpn.md#flake-pilot)    | [❌ no](./scenarios/corporate-vpn.md#flake-pilot)         | [✅ yes](./scenarios/corporate-vpn.md#glaipnir) | [✅ yes‡](./scenarios/corporate-vpn.md#podman)      |
| [Something outside reaches a guest service](./scenarios/inbound.md) | [❌ no](./scenarios/inbound.md#vivarium)              | [⚠️ partial](./scenarios/inbound.md#flake-pilot)          | [✅ yes](./scenarios/inbound.md#flake-pilot)              | [❌ no](./scenarios/inbound.md#glaipnir)        | [✅ yes](./scenarios/inbound.md#podman)             |

## Guest environment

The guest is an artifact somebody produced. These rows ask how that artifact is defined and assembled, what goes inside it and how the project's own tooling meets it, whether the definition travels to another person and repeats there, what ran while it was being produced, and how you get one in the first place.

| capability                                                                                 | `vivarium`                                              | `flake-pilot` firecracker                                | `flake-pilot` krun                                       | `glaipnir`                                            | `podman`                                            |
| ------------------------------------------------------------------------------------------ | ------------------------------------------------------- | -------------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------- | --------------------------------------------------- |
| [Defined by a project file](./scenarios/project-file.md)                                   | [✅ yes](./scenarios/project-file.md#vivarium)          | [❌ no](./scenarios/project-file.md#flake-pilot)         | [❌ no](./scenarios/project-file.md#flake-pilot)         | [❌ no](./scenarios/project-file.md#glaipnir)         | [⚠️ partial](./scenarios/project-file.md#podman)     |
| [The project's own dev environment loads when you enter](./scenarios/inner-environment.md) | [✅ yes*](./scenarios/inner-environment.md#vivarium)    | [❌ no](./scenarios/inner-environment.md#flake-pilot)    | [✅ yes‡](./scenarios/inner-environment.md#flake-pilot)  | [✅ yes‡](./scenarios/inner-environment.md#glaipnir)  | [✅ yes‡](./scenarios/inner-environment.md#podman)  |
| [Compose the environment from separate, reusable parts](./scenarios/composition.md)        | [✅ yes](./scenarios/composition.md#vivarium)           | [✅ yes](./scenarios/composition.md#flake-pilot)         | [✅ yes](./scenarios/composition.md#flake-pilot)         | [✅ yes](./scenarios/composition.md#glaipnir)         | [❌ no](./scenarios/composition.md#podman)          |
| [A collision between two parts is reported](./scenarios/collision.md)                      | [✅ yes](./scenarios/collision.md#vivarium)             | [❌ no](./scenarios/collision.md#flake-pilot)            | [❌ no](./scenarios/collision.md#flake-pilot)            | [➖ n/a](./scenarios/collision.md#glaipnir)           | [➖ n/a](./scenarios/collision.md#podman)           |
| [There is a shareable unit smaller than the whole environment](./scenarios/config-unit.md) | [✅ yes](./scenarios/config-unit.md#vivarium)           | [✅ yes](./scenarios/config-unit.md#flake-pilot)         | [✅ yes](./scenarios/config-unit.md#flake-pilot)         | [❌ no](./scenarios/config-unit.md#glaipnir)          | [❌ no](./scenarios/config-unit.md#podman)          |
| [A shared unit's portability is enforced](./scenarios/portability-enforced.md)             | [✅ yes](./scenarios/portability-enforced.md#vivarium)  | [❌ no](./scenarios/portability-enforced.md#flake-pilot) | [❌ no](./scenarios/portability-enforced.md#flake-pilot) | [❌ no](./scenarios/portability-enforced.md#glaipnir) | [❌ no](./scenarios/portability-enforced.md#podman) |
| [The same definition gives everyone the same environment](./scenarios/same-definition.md)  | [✅ yes](./scenarios/same-definition.md#vivarium)       | [⚠️ partial](./scenarios/same-definition.md#flake-pilot)  | [❌ no](./scenarios/same-definition.md#flake-pilot)      | [❌ no](./scenarios/same-definition.md#glaipnir)      | [✅ yes‡](./scenarios/same-definition.md#podman)    |
| [Choose what goes in the guest, and set it up at build](./scenarios/own-setup-at-build.md) | [⚠️ partial](./scenarios/own-setup-at-build.md#vivarium) | [✅ yes](./scenarios/own-setup-at-build.md#flake-pilot)  | [✅ yes](./scenarios/own-setup-at-build.md#flake-pilot)  | [✅ yes](./scenarios/own-setup-at-build.md#glaipnir)  | [✅ yes](./scenarios/own-setup-at-build.md#podman)  |
| [Run your own setup at every start](./scenarios/own-setup-at-start.md)                     | [✅ yes](./scenarios/own-setup-at-start.md#vivarium)    | [✅ yes‡](./scenarios/own-setup-at-start.md#flake-pilot) | [✅ yes‡](./scenarios/own-setup-at-start.md#flake-pilot) | [✅ yes](./scenarios/own-setup-at-start.md#glaipnir)  | [✅ yes‡](./scenarios/own-setup-at-start.md#podman) |
| [The build runs no user-supplied commands as root](./scenarios/build-steps.md)             | [✅ yes](./scenarios/build-steps.md#vivarium)           | [❌ no](./scenarios/build-steps.md#flake-pilot)          | [❌ no](./scenarios/build-steps.md#flake-pilot)          | [❌ no](./scenarios/build-steps.md#glaipnir)          | [❌ no](./scenarios/build-steps.md#podman)          |
| [Choose the guest operating system](./scenarios/guest-os.md)                               | [❌ no†](./scenarios/guest-os.md#vivarium)              | [✅ yes](./scenarios/guest-os.md#flake-pilot)            | [✅ yes](./scenarios/guest-os.md#flake-pilot)            | [❌ no](./scenarios/guest-os.md#glaipnir)             | [✅ yes](./scenarios/guest-os.md#podman)            |
| [Pull a prebuilt image](./scenarios/prebuilt-image.md)                                     | [❌ no†](./scenarios/prebuilt-image.md#vivarium)        | [✅ yes](./scenarios/prebuilt-image.md#flake-pilot)      | [✅ yes](./scenarios/prebuilt-image.md#flake-pilot)      | [✅ yes](./scenarios/prebuilt-image.md#glaipnir)      | [✅ yes](./scenarios/prebuilt-image.md#podman)      |

## Living with it

What it costs to use every day.

| capability                                                                                     | `vivarium`                                            | `flake-pilot` firecracker                               | `flake-pilot` krun                                      | `glaipnir`                                           | `podman`                                           |
| ---------------------------------------------------------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------- |
| [A later command reaches the instance already running](./scenarios/later-command.md)           | [✅ yes](./scenarios/later-command.md#vivarium)       | [✅ yes](./scenarios/later-command.md#flake-pilot)      | [❌ no](./scenarios/later-command.md#flake-pilot)       | [❌ no](./scenarios/later-command.md#glaipnir)       | [❌ no](./scenarios/later-command.md#podman)       |
| [A second session joins it while the first is still there](./scenarios/concurrent-sessions.md) | [✅ yes](./scenarios/concurrent-sessions.md#vivarium) | [❌ no](./scenarios/concurrent-sessions.md#flake-pilot) | [❌ no](./scenarios/concurrent-sessions.md#flake-pilot) | [❌ no](./scenarios/concurrent-sessions.md#glaipnir) | [❌ no](./scenarios/concurrent-sessions.md#podman) |
| [Installs from a distro package](./scenarios/install.md)                                       | [❌ no](./scenarios/install.md#vivarium)              | ✅ yes                                                  | ✅ yes                                                  | ✅ yes                                               | ✅ yes                                             |
| [Feels like a native command](./scenarios/native-command.md)                                   | [❌ no](./scenarios/native-command.md#vivarium)       | ✅ yes                                                  | ✅ yes                                                  | [❌ no](./scenarios/native-command.md#glaipnir)      | ❌ no                                              |

## What accumulates

Builds, sandboxes, and disk pile up on a machine that gets used. These rows ask whether you can go back to an earlier build, move forward on purpose rather than by surprise, get space back without tearing anything down, and see what exists at all.

| capability                                                                  | `vivarium`                                     | `flake-pilot` firecracker                        | `flake-pilot` krun                                   | `glaipnir`                                     | `podman`                                    |
| --------------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------ | ---------------------------------------------------- | ---------------------------------------------- | ------------------------------------------- |
| [Boot a previous build when the new one is broken](./scenarios/rollback.md) | [✅ yes*](./scenarios/rollback.md#vivarium)    | [❌ no](./scenarios/rollback.md#flake-pilot)     | [❌ no](./scenarios/rollback.md#flake-pilot)         | [❌ no](./scenarios/rollback.md#glaipnir)      | [✅ yes‡](./scenarios/rollback.md#podman)   |
| [Update on purpose](./scenarios/update.md)                                  | [✅ yes*](./scenarios/update.md#vivarium)      | [✅ yes](./scenarios/update.md#flake-pilot)      | [❌ no](./scenarios/update.md#flake-pilot)           | ❌ no                                          | [⚠️ partial](./scenarios/update.md#podman)   |
| [Reclaim disk without a teardown](./scenarios/reclaim-disk.md)              | [❌ no*](./scenarios/reclaim-disk.md#vivarium) | [❌ no](./scenarios/reclaim-disk.md#flake-pilot) | [⚠️ partial](./scenarios/reclaim-disk.md#flake-pilot) | [✅ yes](./scenarios/reclaim-disk.md#glaipnir) | ✅ yes                                      |
| [See every definition on the machine](./scenarios/definitions.md)           | [❌ no*](./scenarios/definitions.md#vivarium)  | [✅ yes](./scenarios/definitions.md#flake-pilot) | [✅ yes](./scenarios/definitions.md#flake-pilot)     | [✅ yes](./scenarios/definitions.md#glaipnir)  | [✅ yes](./scenarios/definitions.md#podman) |
| [See every running instance on the machine](./scenarios/instances.md)       | [❌ no*](./scenarios/instances.md#vivarium)    | [❌ no](./scenarios/instances.md#flake-pilot)    | [❌ no](./scenarios/instances.md#flake-pilot)        | [✅ yes](./scenarios/instances.md#glaipnir)    | [✅ yes](./scenarios/instances.md#podman)   |

## What the second route costs

The two flake-pilot columns agree on most rows and diverge on eleven, and the divergence runs both ways rather than ranking one route above the other. Registration is the same model on both; what changes is what the engine underneath it can and cannot do.

| `krun` buys                                                                | `krun` gives back                                                            |
| -------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| the workspace at its host path, and the choice of paths recorded in a file | a later command, because the engine has no `exec` and so no `--resume`       |
| the project's own toolchain, where the image somebody chose ships one      | update on purpose, and the same definition, because the unit is a moving tag |
| a published port, without an operator building the plumbing first          | the network boundary, because egress leaves through the host's routing table |
| reclaiming disk, through the `%remove` pseudo-argument                     | a refusal on session directories, because now there is a mount to refuse     |
|                                                                            | a deny posture that was the shipped state rather than an option to write in  |

Neither route reaches a row the other closes by design, and both are read at a registration upstream publishes. A reader choosing between them is choosing what to lose.

## How to read the marks

Three kinds of no, one qualified yes, and one mark that is not a verdict at all.

| form      | means                                                                  |
| --------- | ---------------------------------------------------------------------- |
| `❌ no`   | an ordinary gap, no position taken                                     |
| `❌ no*`  | the specification closes it; the code has not caught up                |
| `❌ no†`  | unreachable by design; the linked cell names what forecloses it        |
| `✅ yes‡` | the tool provides the mechanism; nothing arranges it, so the user does |
| `➖ n/a`  | the question does not arise at this setup; the linked cell says why    |

Read the bare ones first: they are what a comparison written by the subject would have left out. Most `†` cells cite a binding invariant; `Scopes credentials per app out of the box` is the exception, citing a design position no invariant states yet, logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md).

The `‡` is the mark that keeps a row honest in the other direction, and it says something specific: the capability is real and reachable by a documented mechanism, and the tool will not do it for you or notice that you forgot. A reader choosing between "it can" and "it does" needs both halves, and a bare yes gives only the first.

The `➖` is scoped exactly as narrowly as every other mark in these tables: it says the question has no answer at the setup that column names, never that the tool has no answer anywhere. `Refuses a mount that would expose the host session` is `➖` at the firecracker route because that route has no mount to refuse, and `❌` at the `krun` route because that route has one and refuses nothing. Two columns, one tool, and the mark that changes is the point of reading both.

Which fixes what a `❌` may mean. It means no mechanism reaches the capability at the setup this table reads — not that the tool declined to perform a step itself. Every subject here delegates: vivarium's build is Nix's, flake-pilot's is KIWI's, glaipnir's and podman's are the container build's, and a row scored on which program executed would be measuring where a project drew its own boundary rather than what a user can do. So the test is what the user can reach without editing the tool: a mechanism the tool provides but leaves unarranged is `‡`, a mechanism the user would have to supply themselves is a `❌`, and a capability reachable only by editing the tool's own source is a `❌` too. Where a cell turns on that distinction it says so.

Configuration a tool's own documentation tells you to write before first use is inside that test rather than outside it, which is why every scenario's method registers or configures the subject first and then starts it. A decision recorded once at registration is not the same capability as the same decision typed at every start, and the rows keep them apart: [choosing mounts](./scenarios/choosing-mounts.md) asks where the decision is written down, and [the workspace row](./scenarios/workspace-mount.md) asks whether the tool needed to be told at all.

A `⚠️ partial` is rare and always a genuine middle rather than a compressed pair — a mechanism that exists and stops short, or a half-built one. Where the hedge was two capabilities wearing one symbol, the row was split instead; [`feature-sweep.md`](./feature-sweep.md) records which rows moved and what each half was hiding.

Many of the rows above were first found in a project other than vivarium rather than in this one; [`feature-sweep.md`](./feature-sweep.md) records which, and why that ratio is the check on whether the sweep was independent.

## What moves at another backend

Every row above is read inside one isolation class, and a capability reachable only outside it is not credited. These are the verdicts that would move at a weaker boundary.

| tool          | weaker backend                             | verdicts that move                                                                                                                                                                                                                         |
| ------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `flake-pilot` | `podman-pilot` at `crun`, podman's default | a later command ❌ → ✅ (`--resume`) and a second session ❌ → ⚠️ (`--attach`, one stream rather than two sessions); everything else the `krun` column already records, since the two share a registration model and differ in the boundary |
| `glaipnir`    | rootless container, its default            | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`, or `podman start -ai`); macOS ❌ → ✅                                                                                                                                 |
| `podman`      | its default runtime, `crun`                | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`); macOS and Windows ❌ → ✅                                                                                                                                            |
| `vivarium`    | none                                       | —                                                                                                                                                                                                                                          |

It moves both ways. A weaker boundary buys re-entry and a second operating system; it costs the rows about what the definition pins. What it does not buy is the mount rows — those are already reachable inside the class, at the `krun` route, which is why they are scored there rather than exiled here.

## Verified

Every table above was read at one date, at these revisions:[^read]

| subject                   | at        |
| ------------------------- | --------- |
| `vivarium`                | `ceb0027` |
| `flake-pilot` firecracker | `920f41e` |
| `flake-pilot` krun        | `920f41e` |
| `glaipnir`                | `21ef389` |
| `podman`                  | 5.x       |

[^read]: Verified: 2026-08-19. Four `flake-pilot` cells — the two kernel rows, engine selection, and default-deny egress — were re-read at `main` on 2026-08-20 against a fresh clone, which removed an engine this table had named on the authority of a source no reader can reach. The `krun` column was added on 2026-08-21 from a fresh clone at the same `920f41e`, together with the adjacent upstream documentation for libkrun, `crun`, and passt that the network and mount rows at that route rest on. [`sources.md`](./sources.md) records that, and carries the provenance for each subject and the re-verification cadence a stale table is caught by.
