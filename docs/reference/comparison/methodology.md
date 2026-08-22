# Comparison methodology

How the verdicts in [`README.md`](./README.md) were reached, and what each mark is allowed to mean. That page carries the tables and stays short; everything that explains them is here. Row derivation is [`feature-sweep.md`](./feature-sweep.md), provenance is [`sources.md`](./sources.md). Every page in this directory carries the revision and date it was read at in a footnote at its foot, so a verdict opens on the verdict rather than on where it came from.

## One isolation class, engines allowed to vary

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

## Building and running are different jobs

A second distinction runs underneath the build rows, and it separates the subjects differently. vivarium produces the guest: a manifest and a lock resolve to a store output the tool itself builds. flake-pilot does not build anything. Its whole `flake-ctl` surface is `pull`, `load`, `register`, `show`, `remove`, `init`, and `list`, and upstream states the delegation as a position rather than leaving it as a gap, naming the Open Build Service with KIWI as one option among the different ways an image can be built and anchoring trust at the image source instead. So a build row here is scored on the artifact chain a user actually follows, not on the tool's own code, and it is scored in both directions: a capability the documented flow reaches counts as reached whoever performs the build, which is how flake-pilot takes yes on packages, setup, and the guest operating system; a guarantee about that build is charged the same way, because an image assembled by arbitrary root commands is what the user receives whoever ran them. Where the answer is that a builder outside the tool owns the question, the cell carries `†` and says so.

## One row asks one thing

One row asks one thing. Where a cell would have to hedge because two capabilities sat under one label, the row is split until each verdict is a single claim, and the two halves link to each other. That is why several rows come in pairs, and why `⚠️ partial` is rare: a hedge here means the answer really is in between, not that the question was two questions.

## Reading the marks

Three kinds of no, one qualified yes, and one mark that is not a verdict at all.

| form      | means                                                                                        |
| --------- | -------------------------------------------------------------------------------------------- |
| `❌ no`   | an ordinary gap, no position taken                                                           |
| `❌ no*`  | the specification closes it; the code has not caught up                                      |
| `❌ no†`  | by design, not by omission; the linked cell names what forecloses it, or who owns it instead |
| `✅ yes‡` | the tool provides the mechanism; nothing arranges it, so the user does                       |
| `➖ n/a`  | the question does not arise at this setup; the linked cell says why                          |

Read the bare ones first: they are what a comparison written by the subject would have left out. A `†` has to name its authority, and three kinds appear: most cells cite a binding invariant, the two build rows cite the tool that owns the job instead, and `Scopes credentials per app out of the box` cites a design position no invariant states yet, logged as `Q-031` in [`open-questions.md`](../../plan/open-questions.md). `The tool derives the workspace mount` cites a recorded decision rather than any of the three, because it names a position vivarium adopted after these tables were first written.

The `‡` is the mark that keeps a row honest in the other direction, and it says something specific: the capability is real and reachable by a documented mechanism, and the tool will not do it for you or notice that you forgot. A reader choosing between "it can" and "it does" needs both halves, and a bare yes gives only the first.

The `➖` is scoped exactly as narrowly as every other mark: it says the question has no answer at the setup that column names, never that the tool has no answer anywhere. `Refuses a mount that would expose the host session` is `➖` at the firecracker route because that route has no mount to refuse, and `❌` at the `krun` route because that route has one and refuses nothing. Two columns, one tool, and the mark that changes is the point of reading both.

Which fixes what a `❌` may mean. It means no mechanism reaches the capability at the setup this table reads — not that the tool declined to perform a step itself. Every subject here delegates: vivarium's build is Nix's, flake-pilot's is KIWI's, glaipnir's and podman's are the container build's, and a row scored on which program executed would be measuring where a project drew its own boundary rather than what a user can do. So the test is what the user can reach without editing the tool: a mechanism the tool provides but leaves unarranged is `‡`, a mechanism the user would have to supply themselves is a `❌`, and a capability reachable only by editing the tool's own source is a `❌` too. Where a cell turns on that distinction it says so.

Configuration a tool's own documentation tells you to write before first use is inside that test rather than outside it, which is why every scenario's method registers or configures the subject first and then starts it. A decision recorded once at registration is not the same capability as the same decision typed at every start, and the rows keep them apart: [choosing mounts](./scenarios/choosing-mounts.md) asks where the decision is written down, and [the workspace row](./scenarios/workspace-mount.md) asks whether the tool needed to be told at all.

A `⚠️ partial` is rare and always a genuine middle rather than a compressed pair — a mechanism that exists and stops short, or a half-built one. Where the hedge was two capabilities wearing one symbol, the row was split instead; [`feature-sweep.md`](./feature-sweep.md) records which rows moved and what each half was hiding.

Many of the rows were first found in a project other than vivarium rather than in this one; [`feature-sweep.md`](./feature-sweep.md) records which, and why that ratio is the check on whether the sweep was independent.

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

## What moves at another backend

Every row above is read inside one isolation class, and a capability reachable only outside it is not credited. These are the verdicts that would move at a weaker boundary.

| tool          | weaker backend                             | verdicts that move                                                                                                                                                                                                                         |
| ------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `flake-pilot` | `podman-pilot` at `crun`, podman's default | a later command ❌ → ✅ (`--resume`) and a second session ❌ → ⚠️ (`--attach`, one stream rather than two sessions); everything else the `krun` column already records, since the two share a registration model and differ in the boundary |
| `glaipnir`    | rootless container, its default            | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`, or `podman start -ai`); macOS ❌ → ✅                                                                                                                                 |
| `podman`      | its default runtime, `crun`                | a later command ❌ → ✅ and a second session ❌ → ✅ (`podman exec`); macOS and Windows ❌ → ✅                                                                                                                                            |
| `vivarium`    | none                                       | —                                                                                                                                                                                                                                          |

It moves both ways. A weaker boundary buys re-entry and a second operating system; it costs the rows about what the definition pins. What it does not buy is the mount rows — those are already reachable inside the class, at the `krun` route, which is why they are scored there rather than exiled here.
