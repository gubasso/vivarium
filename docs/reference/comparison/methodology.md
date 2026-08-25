# Comparison methodology

How the verdicts in [`README.md`](./README.md) were reached, and what each mark is allowed to mean. That page carries the tables; this one explains them. Row derivation is [`feature-sweep.md`](./feature-sweep.md), provenance is [`sources.md`](./sources.md). Every page here carries the revision and date it was read at in a footnote at its foot.

## One isolation class, engines allowed to vary

Every row is read at one isolation class for every tool: a microVM behind a hardware-virtualization boundary with its own guest kernel, the only class vivarium has. The engine inside that class is not held fixed, because the subjects do not agree on one — firecracker for one column, libkrun for two, Kata for one. Holding the class fixed and letting the engine vary is the claim these tables can defend.

| subject                   | read at                                                                          |
| ------------------------- | -------------------------------------------------------------------------------- |
| `vivarium`                | its one microVM                                                                  |
| `flake-pilot` firecracker | `firecracker-pilot`, at the upstream `claude` firecracker registration           |
| `flake-pilot` krun        | `podman-pilot` at `--runtime krun`, at the upstream `claude` `krun` registration |
| `glaipnir`                | the libkrun microVM                                                              |
| `bunkerbox`               | its one Kata container, at `io.containerd.kata.v2`                               |

flake-pilot forces the two-column split: it reaches the class by two routes, upstream publishes a `claude` registration for each, and [the engine row](./scenarios/engine-choice.md#flake-pilot) records that upstream states the two are not equivalent. So a flake-pilot verdict always says which route it is about.

## Building and running are different jobs

A second distinction runs under the build rows. vivarium produces the guest: a manifest and a lock resolve to a store output the tool builds. flake-pilot builds nothing — its whole `flake-ctl` surface is `pull`, `load`, `register`, `show`, `remove`, `init`, and `list` — and upstream states that delegation as a position, naming the Open Build Service with KIWI and anchoring trust at the image source. bunkerbox builds an OCI archive from a Containerfile the image author writes.

So a build row is scored on the artifact chain a user follows, not on the tool's own code, and it is scored in both directions: a capability the documented flow reaches counts as reached whoever performs the build, and a guarantee about that build is charged the same way, because an image assembled by arbitrary root commands is what the user receives whoever ran them. Where a builder outside the tool owns the question, the cell carries `†` and says so.

## One row asks one thing

Where a cell would hedge because two capabilities sat under one label, the row is split until each verdict is a single claim, and the halves link to each other. That is why several rows come in pairs, and why `⚠️ partial` is rare: a hedge here means the answer is genuinely in between, not that the question was two questions.

## One subject is enough

A row is admitted when at least one subject has an answer a reader would decide on. It was two until 2026-08-25; the relaxation is recorded in [`feature-sweep.md`](./feature-sweep.md) with what it re-admitted.

The looser rule is easy to abuse in one direction: vivarium-only capabilities now qualify, and a table whose rows all originate with the subject is a scorecard rather than a comparison. Two things hold it honest. The second half of the rule did not move — an entry must still be something a reader decides about rather than an implementation detail — and [`feature-sweep.md`](./feature-sweep.md) states, on every refresh, how many rows were first seen in a project other than vivarium. If that ratio collapses, the sweep was not independent.

## Reading the marks

Three kinds of no, one qualified yes, and one mark that is not a verdict at all.

| form      | means                                                                                        |
| --------- | -------------------------------------------------------------------------------------------- |
| `❌ no`   | an ordinary gap, no position taken                                                           |
| `❌ no*`  | the specification closes it; the code has not caught up                                      |
| `❌ no†`  | by design, not by omission; the linked cell names what forecloses it, or who owns it instead |
| `✅ yes‡` | the tool provides the mechanism; nothing arranges it, so the user does                       |
| `➖ n/a`  | the question does not arise at this setup; the linked cell says why                          |

Read the bare ones first: they are what a comparison written by the subject would have left out.

A `†` names its authority. Most cells cite a binding invariant; the build rows cite the tool that owns the job instead; `Scopes credentials per app out of the box` cites a design position no invariant states yet, logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md); and `Mounts the project you start it in` cites a recorded decision, because it names a position vivarium adopted after these tables were first written.

A `‡` keeps a row honest in the other direction: the capability is real and reachable by a documented mechanism, and the tool will not do it for you or notice that you forgot. A reader choosing between "it can" and "it does" needs both halves.

A `➖` is scoped as narrowly as every other mark: the question has no answer at the setup that column names, never that the tool has no answer anywhere. `Refuses a mount that would expose the host session` is `➖` at the firecracker route because that route has no mount to refuse, and `❌` at the `krun` route because that route has one and refuses nothing.

Which fixes what a `❌` may mean. It means no mechanism reaches the capability at the setup this table reads — not that the tool declined to perform a step itself. Every subject delegates: vivarium's build is Nix's, flake-pilot's is KIWI's, glaipnir's and bunkerbox's are the container build's. So the test is what the user can reach without editing the tool: a mechanism the tool provides but leaves unarranged is `‡`, a mechanism the user would supply themselves is `❌`, and a capability reachable only by editing the tool's source is `❌` too.

Configuration a tool's own documentation tells you to write before first use is inside that test rather than outside it, which is why every scenario configures the subject first and then starts it. A decision recorded once at registration is not the same capability as the same decision typed at every start: [choosing mounts](./scenarios/choosing-mounts.md) asks where the decision is written down, and [the workspace row](./scenarios/workspace-mount.md) asks whether the tool needed to be told at all.

## What the second route costs

The two flake-pilot columns agree on most rows and diverge on eleven, and the divergence runs both ways rather than ranking one route above the other. Registration is the same model on both; what changes is what the engine underneath can do.

| `krun` buys                                                                | `krun` gives back                                                            |
| -------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| the workspace at its host path, and the choice of paths recorded in a file | a later command, because the engine has no `exec` and so no `--resume`       |
| the project's own toolchain, where the image somebody chose ships one      | update on purpose, and the same definition, because the unit is a moving tag |
| a published port, without an operator building the plumbing first          | the network boundary, because egress leaves through the host's routing table |
| reclaiming disk, through the `%remove` pseudo-argument                     | a refusal on session directories, because now there is a mount to refuse     |
|                                                                            | a deny posture that was the shipped state rather than an option to write in  |

Neither route reaches a row the other closes by design, and both are read at a registration upstream publishes. A reader choosing between them is choosing what to lose.

## What moves at another backend

Every row above is read inside one isolation class, and a capability reachable only outside it is not credited. These are the verdicts that would move at a weaker boundary.

| tool          | weaker backend                             | verdicts that move                                                                                                                                                                                                                         |
| ------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `flake-pilot` | `podman-pilot` at `crun`, podman's default | a later command ❌ → ✅ (`--resume`) and a second session ❌ → ⚠️ (`--attach`, one stream rather than two sessions); everything else the `krun` column already records, since the two share a registration model and differ in the boundary |
| `glaipnir`    | rootless container, its default            | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`, or `podman start -ai`); macOS ❌ → ✅                                                                                                                                 |
| `bunkerbox`   | none                                       | — its only runtime is `io.containerd.kata.v2`, named in code                                                                                                                                                                               |
| `vivarium`    | none                                       | —                                                                                                                                                                                                                                          |

It moves both ways. A weaker boundary buys re-entry and a second operating system; it costs the rows about what the definition pins. What it does not buy is the mount rows — those are already reachable inside the class, at the `krun` route, which is why they are scored there rather than exiled here.

The row bunkerbox adds is the mirror of this table rather than an entry in it. Its boundary does not weaken by backend; part of the workload leaves through [passthrough](./scenarios/host-toolchain.md), which is scored inside the tables because it is reachable at the setup this column names.
