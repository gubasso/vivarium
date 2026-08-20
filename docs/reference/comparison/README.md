# vivarium and the alternatives

Four tools that put a coding agent somewhere it cannot wreck your laptop. A capability name links to the scenario that decides it; a verdict links to what that tool did when the scenario was run against it. Row derivation: [`feature-sweep.md`](./feature-sweep.md). Provenance: [`sources.md`](./sources.md). The same work end to end: [`walkthroughs.md`](./walkthroughs.md).

## Methodology

Every capability row is read at the same backend for every tool: the microVM, the only backend vivarium has. One fixed setup per tool, used in every row, method, and walkthrough:

| tool          | read at                                                    |
| ------------- | ---------------------------------------------------------- |
| `vivarium`    | its one microVM                                            |
| `flake-pilot` | `firecracker-pilot`, at the upstream `claude` registration |
| `glaipnir`    | the libkrun microVM                                        |
| `podman`      | `podman run --runtime krun`                                |

One row asks one thing. Where a cell would have to hedge because two capabilities sat under one label, the row is split until each verdict is a single claim, and the two halves link to each other. That is why several rows come in pairs, and why `⚠️ partial` is rare: a hedge here means the answer really is in between, not that the question was two questions.

Legend: ✅ yes · ⚠️ partial · ❌ no · ➖ n/a

- `*` — specified, not built yet
- `†` — deliberately not planned; the cell links to what forecloses it
- `‡` — the mechanism is the tool's and the arrangement is yours; it works, and it works each time because you invoked it

## Isolation backends

What each tool can run the workload on, and how the backend gets chosen. This is the only cross-backend section.

| backend question                                                                                                          | `vivarium`                                                          | `flake-pilot`                                                     | `glaipnir`                                          | `podman`                                                |
| ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ----------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------- |
| [MicroVM, own guest kernel](./scenarios.md#own-kernel)                                                                    | ✅ only mode                                                        | [✅ firecracker, `krun`](./scenarios.md#own-kernel-flake-pilot)   | [✅ libkrun](./scenarios.md#own-kernel-glaipnir)    | [✅ `--runtime krun`](./scenarios.md#own-kernel-podman) |
| [OCI container, shared kernel](./scenarios.md#shared-kernel-isolation-with-an-oci-runtime)                                | [❌ no†](./scenarios.md#shared-kernel-vivarium)                     | [✅ `crun`](./scenarios.md#shared-kernel-flake-pilot)             | [✅ default](./scenarios.md#shared-kernel-glaipnir) | ✅ default                                              |
| [Who selects the backend](./scenarios.md#choose-the-engine)                                                               | [nobody — capability class†](./scenarios.md#engine-choice-vivarium) | [user, at registration](./scenarios.md#engine-choice-flake-pilot) | [host probe](./scenarios.md#engine-choice-glaipnir) | user, per call                                          |
| [Host without KVM](./scenarios.md#runs-on-a-host-without-kvm)                                                             | [refuses](./scenarios.md#no-kvm-vivarium)                           | [container backends only](./scenarios.md#no-kvm-flake-pilot)      | [falls back, warns](./scenarios.md#no-kvm-glaipnir) | [container runtimes only](./scenarios.md#no-kvm-podman) |
| [No flag selects a weaker boundary](./scenarios.md#no-flag-selects-a-weaker-boundary)                                     | [✅ yes](./scenarios.md#boundary-flag-vivarium)                     | [✅ yes](./scenarios.md#boundary-flag-flake-pilot)                | [❌ no](./scenarios.md#boundary-flag-glaipnir)      | [❌ no](./scenarios.md#boundary-flag-podman)            |
| [No file you did not write selects a weaker boundary](./scenarios.md#no-file-you-did-not-write-selects-a-weaker-boundary) | [✅ yes](./scenarios.md#boundary-file-vivarium)                     | [❌ no](./scenarios.md#boundary-file-flake-pilot)                 | [✅ yes](./scenarios.md#boundary-file-glaipnir)     | [❌ no](./scenarios.md#boundary-file-podman)            |
| [macOS](./scenarios.md#runs-on-macos)                                                                                     | [❌ no](./scenarios.md#macos-vivarium)                              | ❌ no                                                             | [container only](./scenarios.md#macos-glaipnir)     | [container only](./scenarios.md#macos-podman)           |

## Data isolation

What crosses the boundary, and what the tool can see once it is inside.

| capability                                                                                                                                                         | `vivarium`                                             | `flake-pilot`                                            | `glaipnir`                                             | `podman`                                            |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------ | -------------------------------------------------------- | ------------------------------------------------------ | --------------------------------------------------- |
| [The tool arranges the workspace mount](./scenarios.md#the-tool-arranges-the-workspace-mount)                                                                      | [✅ yes](./scenarios.md#workspace-mount-vivarium)      | [❌ no](./scenarios.md#workspace-mount-flake-pilot)      | [✅ yes](./scenarios.md#workspace-mount-glaipnir)      | [❌ no](./scenarios.md#workspace-mount-podman)      |
| [Work stays at its host path](./scenarios.md#work-stays-at-its-host-path)                                                                                          | [✅ yes](./scenarios.md#host-path-vivarium)            | [➖ n/a](./scenarios.md#host-path-flake-pilot)           | [❌ no](./scenarios.md#host-path-glaipnir)             | [✅ yes‡](./scenarios.md#host-path-podman)          |
| [Choose which host paths cross, in a file rather than on the command line](./scenarios.md#choose-which-host-paths-cross-in-a-file-rather-than-on-the-command-line) | [✅ yes](./scenarios.md#choosing-mounts-vivarium)      | [➖ n/a](./scenarios.md#choosing-mounts-flake-pilot)     | [❌ no](./scenarios.md#choosing-mounts-glaipnir)       | [✅ yes‡](./scenarios.md#choosing-mounts-podman)    |
| [The caller chooses which host environment variables cross](./scenarios.md#the-caller-chooses-which-host-environment-variables-cross)                              | [✅ yes](./scenarios.md#environment-vivarium)          | [✅ yes](./scenarios.md#environment-flake-pilot)         | [❌ no](./scenarios.md#environment-glaipnir)           | ✅ yes                                              |
| [Refuses a mount that would expose the host session](./scenarios.md#refuses-a-mount-that-would-expose-the-host-session)                                            | [✅ yes](./scenarios.md#session-sockets-vivarium)      | [➖ n/a](./scenarios.md#session-sockets-flake-pilot)     | ❌ no                                                  | ❌ no                                               |
| [Use an SSH or GPG key without the key entering the sandbox](./scenarios.md#use-an-ssh-or-gpg-key-without-the-key-entering-the-sandbox)                            | [✅ yes](./scenarios.md#key-material-vivarium)         | [❌ no](./scenarios.md#key-material-flake-pilot)         | [❌ no](./scenarios.md#key-material-glaipnir)          | [❌ no](./scenarios.md#key-material-podman)         |
| [Secrets are kept out of the built artifact](./scenarios.md#secrets-are-kept-out-of-the-built-artifact)                                                            | [✅ yes](./scenarios.md#secrets-in-the-build-vivarium) | ❌ no                                                    | [✅ yes](./scenarios.md#secrets-in-the-build-glaipnir) | ❌ no                                               |
| [Keeping secrets out of the build is enforced](./scenarios.md#keeping-secrets-out-of-the-build-is-enforced)                                                        | [⚠️ partial](./scenarios.md#secrets-enforced-vivarium)  | ❌ no                                                    | [❌ no](./scenarios.md#secrets-enforced-glaipnir)      | ❌ no                                               |
| [Commit an encrypted secret alongside the config](./scenarios.md#commit-an-encrypted-secret-alongside-the-config)                                                  | [✅ yes‡](./scenarios.md#shipping-a-secret-vivarium)   | [❌ no](./scenarios.md#shipping-a-secret-flake-pilot)    | [❌ no](./scenarios.md#shipping-a-secret-glaipnir)     | [✅ yes‡](./scenarios.md#shipping-a-secret-podman)  |
| [Scopes credentials per app out of the box](./scenarios.md#scopes-credentials-per-app-out-of-the-box)                                                              | [❌ no†](./scenarios.md#per-tool-credentials-vivarium) | [❌ no](./scenarios.md#per-tool-credentials-flake-pilot) | [✅ yes](./scenarios.md#per-tool-credentials-glaipnir) | [❌ no](./scenarios.md#per-tool-credentials-podman) |

## Network isolation

Where the tool can reach, and who decides. Reachable, not default: vivarium's egress is unrestricted until a manifest says otherwise, and the first row asks whether the deny posture exists at all.

| capability                                                                                            | `vivarium`                                            | `flake-pilot`                                            | `glaipnir`                                      | `podman`                                            |
| ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------- | --------------------------------------------------- |
| [Egress can be default-deny](./scenarios.md#egress-can-be-default-deny)                               | [✅ yes](./scenarios.md#default-deny-egress-vivarium) | [✅ yes](./scenarios.md#default-deny-egress-flake-pilot) | ❌ no                                           | [✅ yes](./scenarios.md#default-deny-egress-podman) |
| [Allowlist by destination name](./scenarios.md#allowlist-by-destination-name)                         | [✅ yes](./scenarios.md#allowlist-vivarium)           | ❌ no                                                    | ❌ no                                           | ❌ no                                               |
| [Stays off a corporate VPN](./scenarios.md#stays-off-a-corporate-vpn)                                 | [❌ no](./scenarios.md#corporate-vpn-vivarium)        | ❌ no                                                    | [✅ yes](./scenarios.md#corporate-vpn-glaipnir) | ❌ no                                               |
| [Something outside reaches a guest service](./scenarios.md#something-outside-reaches-a-guest-service) | [❌ no](./scenarios.md#inbound-vivarium)              | [⚠️ partial](./scenarios.md#inbound-flake-pilot)          | [❌ no](./scenarios.md#inbound-glaipnir)        | [✅ yes](./scenarios.md#inbound-podman)             |

## Guest environment

The guest is an artifact somebody produced. These rows ask how that artifact is defined and assembled, what goes inside it and how the project's own tooling meets it, whether the definition travels to another person and repeats there, what ran while it was being produced, and how you get one in the first place.

| capability                                                                                                                                  | `vivarium`                                             | `flake-pilot`                                            | `glaipnir`                                             | `podman`                                             |
| ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ | -------------------------------------------------------- | ------------------------------------------------------ | ---------------------------------------------------- |
| [Defined by a project file](./scenarios.md#defined-by-a-project-file)                                                                       | ✅ yes                                                 | [❌ no](./scenarios.md#project-file-flake-pilot)         | [❌ no](./scenarios.md#project-file-glaipnir)          | [⚠️ partial](./scenarios.md#project-file-podman)      |
| [Choose which programs are installed in the guest](./scenarios.md#choose-which-programs-are-installed-in-the-guest)                         | [✅ yes](./scenarios.md#programs-installed-vivarium)   | [❌ no](./scenarios.md#programs-installed-flake-pilot)   | [✅ yes](./scenarios.md#programs-installed-glaipnir)   | [✅ yes](./scenarios.md#programs-installed-podman)   |
| [The project's own dev environment loads when you enter](./scenarios.md#the-projects-own-dev-environment-loads-when-you-enter)              | [✅ yes*](./scenarios.md#inner-environment-vivarium)   | [❌ no](./scenarios.md#inner-environment-flake-pilot)    | [⚠️ partial](./scenarios.md#inner-environment-glaipnir) | [⚠️ partial](./scenarios.md#inner-environment-podman) |
| [Compose the environment from separate, reusable parts](./scenarios.md#compose-the-environment-from-separate-reusable-parts)                | [✅ yes](./scenarios.md#composition-vivarium)          | [✅ yes](./scenarios.md#composition-flake-pilot)         | [✅ yes](./scenarios.md#composition-glaipnir)          | [❌ no](./scenarios.md#composition-podman)           |
| [A collision between two parts is reported](./scenarios.md#a-collision-between-two-parts-is-reported)                                       | [✅ yes](./scenarios.md#collision-vivarium)            | [❌ no](./scenarios.md#collision-flake-pilot)            | [➖ n/a](./scenarios.md#collision-glaipnir)            | [➖ n/a](./scenarios.md#collision-podman)            |
| [There is a shareable unit smaller than the whole environment](./scenarios.md#there-is-a-shareable-unit-smaller-than-the-whole-environment) | [✅ yes](./scenarios.md#config-unit-vivarium)          | [✅ yes](./scenarios.md#config-unit-flake-pilot)         | [❌ no](./scenarios.md#config-unit-glaipnir)           | [❌ no](./scenarios.md#config-unit-podman)           |
| [A shared unit's portability is enforced](./scenarios.md#a-shared-units-portability-is-enforced)                                            | [✅ yes](./scenarios.md#portability-enforced-vivarium) | [❌ no](./scenarios.md#portability-enforced-flake-pilot) | [❌ no](./scenarios.md#portability-enforced-glaipnir)  | [❌ no](./scenarios.md#portability-enforced-podman)  |
| [The same definition gives everyone the same environment](./scenarios.md#the-same-definition-gives-everyone-the-same-environment)           | [✅ yes](./scenarios.md#same-definition-vivarium)      | [⚠️ partial](./scenarios.md#same-definition-flake-pilot)  | [❌ no](./scenarios.md#same-definition-glaipnir)       | [✅ yes‡](./scenarios.md#same-definition-podman)     |
| [Run your own setup at build time](./scenarios.md#run-your-own-setup-at-build-time)                                                         | [❌ no†](./scenarios.md#own-setup-at-build-vivarium)   | [❌ no](./scenarios.md#own-setup-at-build-flake-pilot)   | [✅ yes](./scenarios.md#own-setup-at-build-glaipnir)   | [✅ yes](./scenarios.md#own-setup-at-build-podman)   |
| [Run your own setup at every start](./scenarios.md#run-your-own-setup-at-every-start)                                                       | [✅ yes](./scenarios.md#own-setup-at-start-vivarium)   | [❌ no](./scenarios.md#own-setup-at-start-flake-pilot)   | [✅ yes](./scenarios.md#own-setup-at-start-glaipnir)   | [✅ yes‡](./scenarios.md#own-setup-at-start-podman)  |
| [The build runs no user-supplied commands as root](./scenarios.md#the-build-runs-no-user-supplied-commands-as-root)                         | ✅ yes                                                 | ❌ no                                                    | [❌ no](./scenarios.md#build-steps-glaipnir)           | ❌ no                                                |
| [Choose the guest operating system](./scenarios.md#choose-the-guest-operating-system)                                                       | [❌ no†](./scenarios.md#guest-os-vivarium)             | [✅ yes](./scenarios.md#guest-os-flake-pilot)            | [❌ no](./scenarios.md#guest-os-glaipnir)              | [✅ yes](./scenarios.md#guest-os-podman)             |
| [Pull a prebuilt image](./scenarios.md#pull-a-prebuilt-image)                                                                               | [❌ no†](./scenarios.md#prebuilt-image-vivarium)       | ✅ yes                                                   | ✅ yes                                                 | ✅ yes                                               |

## Living with it

What it costs to use every day.

| capability                                                                                                                          | `vivarium`                                            | `flake-pilot`                                           | `glaipnir`                                           | `podman`                                           |
| ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------- |
| [A later command reaches the instance already running](./scenarios.md#a-later-command-reaches-the-instance-already-running)         | [✅ yes](./scenarios.md#later-command-vivarium)       | [✅ yes](./scenarios.md#later-command-flake-pilot)      | [❌ no](./scenarios.md#later-command-glaipnir)       | [❌ no](./scenarios.md#later-command-podman)       |
| [A second session joins it while the first is still there](./scenarios.md#a-second-session-joins-it-while-the-first-is-still-there) | [✅ yes](./scenarios.md#concurrent-sessions-vivarium) | [❌ no](./scenarios.md#concurrent-sessions-flake-pilot) | [❌ no](./scenarios.md#concurrent-sessions-glaipnir) | [❌ no](./scenarios.md#concurrent-sessions-podman) |
| [Installs from a distro package](./scenarios.md#installs-from-a-distro-package)                                                     | [❌ no](./scenarios.md#install-vivarium)              | ✅ yes                                                  | ✅ yes                                               | ✅ yes                                             |
| [Feels like a native command](./scenarios.md#feels-like-a-native-command)                                                           | [❌ no](./scenarios.md#native-command-vivarium)       | ✅ yes                                                  | [❌ no](./scenarios.md#native-command-glaipnir)      | ❌ no                                              |
| [Works for a tool the sandbox has never heard of](./scenarios.md#works-for-a-tool-the-sandbox-has-never-heard-of)                   | [✅ yes](./scenarios.md#any-tool-vivarium)            | [✅ yes](./scenarios.md#any-tool-flake-pilot)           | [❌ no](./scenarios.md#any-tool-glaipnir)            | [✅ yes](./scenarios.md#any-tool-podman)           |

## What accumulates

Builds, sandboxes, and disk pile up on a machine that gets used. These rows ask whether you can go back to an earlier build, move forward on purpose rather than by surprise, get space back without tearing anything down, and see what exists at all.

| capability                                                                                                          | `vivarium`                                     | `flake-pilot`                                    | `glaipnir`                                     | `podman`                                    |
| ------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------ | ---------------------------------------------- | ------------------------------------------- |
| [Boot a previous build when the new one is broken](./scenarios.md#boot-a-previous-build-when-the-new-one-is-broken) | [✅ yes*](./scenarios.md#rollback-vivarium)    | ❌ no                                            | ❌ no                                          | ❌ no                                       |
| [Update on purpose](./scenarios.md#update-on-purpose)                                                               | [✅ yes*](./scenarios.md#update-vivarium)      | [✅ yes](./scenarios.md#update-flake-pilot)      | ❌ no                                          | [⚠️ partial](./scenarios.md#update-podman)   |
| [Reclaim disk without a teardown](./scenarios.md#reclaim-disk-without-a-teardown)                                   | [❌ no*](./scenarios.md#reclaim-disk-vivarium) | [❌ no](./scenarios.md#reclaim-disk-flake-pilot) | [✅ yes](./scenarios.md#reclaim-disk-glaipnir) | ✅ yes                                      |
| [See every definition on the machine](./scenarios.md#see-every-definition-on-the-machine)                           | [❌ no*](./scenarios.md#definitions-vivarium)  | [✅ yes](./scenarios.md#definitions-flake-pilot) | [✅ yes](./scenarios.md#definitions-glaipnir)  | [✅ yes](./scenarios.md#definitions-podman) |
| [See every running instance on the machine](./scenarios.md#see-every-running-instance-on-the-machine)               | [❌ no*](./scenarios.md#instances-vivarium)    | [❌ no](./scenarios.md#instances-flake-pilot)    | [✅ yes](./scenarios.md#instances-glaipnir)    | [✅ yes](./scenarios.md#instances-podman)   |

## How to read the marks

Three kinds of no, and one qualified yes.

| form      | means                                                                  | in vivarium's column |
| --------- | ---------------------------------------------------------------------- | -------------------- |
| `❌ no`   | an ordinary gap, no position taken                                     | 4                    |
| `❌ no*`  | the specification closes it; the code has not caught up                | 3                    |
| `❌ no†`  | unreachable by design; the linked cell names what forecloses it        | 4                    |
| `✅ yes‡` | the tool provides the mechanism; nothing arranges it, so the user does | 1                    |

Read the bare ones first: they are what a comparison written by the subject would have left out. Three of the four `†` cells cite a binding invariant; the fourth, `Scopes credentials per app out of the box`, cites a design position no invariant states yet, logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md).

The `‡` is the mark that keeps a row honest in the other direction. Six cells carry it, five of them podman's, and it says something specific: the capability is real and reachable by a documented mechanism, and the tool will not do it for you or notice that you forgot. A reader choosing between "it can" and "it does" needs both halves, and a bare yes gives only the first.

Seven `⚠️ partial` cells remain out of one hundred and forty-eight. Each is a genuine middle rather than a compressed pair — a mechanism that exists and stops short, or a half-built one. Where the hedge was two capabilities wearing one symbol, the row was split instead; [`feature-sweep.md`](./feature-sweep.md) records which rows moved and what each half was hiding.

Sixteen of the thirty-seven capability rows, and five of the seven backend rows, were first found in a project other than vivarium.

## What moves at another backend

Every row above is read at one fixed setup per tool, and a capability reachable only elsewhere is not credited. Three subjects have somewhere else to be read; these are the verdicts that would move there.

| tool          | other backend                           | verdicts that move                                                                                                                                                                                                                                                                                         |
| ------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `flake-pilot` | `podman-pilot`, at podman's own runtime | a second session ❌ → ✅ (`--attach`); work at its host path ➖ → ⚠️ and the mount arrangement ❌ → ⚠️ (`--volume`, a quarantine directory rather than the project tree); session sockets ➖ → ❌; reclaim disk ❌ → ⚠️ (`%remove`); the same definition ⚠️ → ❌ and update ✅ → ❌ (`:latest`, rebuilt daily) |
| `glaipnir`    | rootless container, its default         | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`, or `podman start -ai`); macOS ❌ → ✅                                                                                                                                                                                                 |
| `podman`      | its default runtime, `crun`             | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`); macOS and Windows ❌ → ✅                                                                                                                                                                                                            |
| `vivarium`    | none                                    | —                                                                                                                                                                                                                                                                                                          |

It moves both ways. A weaker boundary buys re-entry, bind mounts, and a second operating system; it costs the rows about what the definition pins.

## Verified

Every table above was read at one date, at these revisions:

| subject       | at        |
| ------------- | --------- |
| `vivarium`    | `ceb0027` |
| `flake-pilot` | `main`    |
| `glaipnir`    | `21ef389` |
| `podman`      | 5.x       |

Verified: 2026-08-19. Four `flake-pilot` cells — the two kernel rows, engine selection, and default-deny egress — were re-read at `main` on 2026-08-20 against a fresh clone, which removed an engine this table had named on the authority of a source no reader can reach. [`sources.md`](./sources.md) records that, and carries the provenance for each subject and the re-verification cadence a stale table is caught by.
