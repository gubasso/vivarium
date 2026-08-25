# vivarium and bunkerbox

[bunkerbox](https://github.com/tinythings/bunkerbox) is the closest thing to vivarium in [the tables](./README.md): same isolation class, same audience, same language, and the same conviction that a coding agent belongs behind a kernel it does not own. It is also the subject that disagrees with vivarium most, and the disagreement reduces to one question — where does a build command run. This page is that one question and what each side pays for its answer. Everything else is a row, and a row is where it stays.

Read at `bunkerbox` `b7f14f3`, per [`sources.md`](./sources.md). It describes itself as a proof of concept.

## Orientation

|                            | `vivarium`                                    | `bunkerbox`                               |
| -------------------------- | --------------------------------------------- | ----------------------------------------- |
| Guest definition           | a TOML manifest compiled to a generated flake | an OCI image built from a `containerfile` |
| Backend                    | a microVM, named by capability class          | Kata Containers over containerd           |
| Unit of packaging          | a project                                     | a tool                                    |
| Where a build command runs | inside the guest                              | on the host                               |
| What pins the environment  | a lockfile                                    | a version string in `build_args`          |
| Cryptography               | none, by rule                                 | AES-256-GCM over the persisted home       |
| Privilege                  | per-user throughout                           | `sudo` for every privileged step          |
| Workspace                  | the host tree, at its host path, uncapped     | an overlay at `/workspace`, capped        |

## The one difference

bunkerbox's guest is a musl Alpine carrying the agent and two helper binaries. It ships no compilers, deliberately: installing the toolchain in the container, upstream reasons, defeats the purpose of the container. So when the agent runs `cargo test`, [passthrough](./scenarios/host-toolchain.md#bunkerbox) proxies the call back out over vsock to a host daemon, which runs the real `cargo` — on the host, under the host kernel — inside the overlay workspace, and streams the output back.

vivarium runs the same command inside. Its [inner layer](./scenarios/inner-environment.md#vivarium) evaluates the project's own `flake.nix` in the guest, and the reason that is affordable rather than absurd is [ADR-0038](../../decisions/ADR-0038-guest-store-sharing.md) and [ADR-0084](../../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md): the host store is shared read-only into every guest and the inner layer provisions its own store on top of it, so a toolchain the host has already realised costs a mount rather than a rebuild.

Two designs, one need, opposite directions:

```text
bunkerbox    agent → vsock → host daemon → bubblewrap? → cargo        (host kernel)
vivarium     agent → cargo                                            (guest kernel)
```

What each buys:

- bunkerbox works with the toolchain you already have, in whatever shape your distribution put it, with no requirement that it be describable in anything.
- vivarium keeps one boundary at one strength, and pays for it by requiring the toolchain to be expressible in Nix.

What each pays is where it gets interesting, because bunkerbox's cost is not the passthrough itself. It is the two defaults around it. `passthrough` is auto-detected and pre-filled on first run from the build-system files in the repository, and `profiles` — the bubblewrap sandbox that would confine what it proxies — is empty. Upstream states the consequence in its own guide: without a sandbox, `cargo build` runs as you, and an agent that can run arbitrary cargo commands can run arbitrary code through `build.rs` or a proc macro. `project.env` compounds it, defaulting to `relaxed`, which forwards the guest's environment to the host command; `paranoid` is the setting upstream names for stopping the agent setting `LD_PRELOAD` or `RUSTFLAGS`.

Configured, the picture is much better: a profile is a real confinement, network-namespaced, with only declared binaries and paths visible. Unconfigured is the state a first run leaves you in, which is what [the boundary-file row](./scenarios/boundary-file.md#bunkerbox) scores.

## What bunkerbox does better

- [Capped writes](./scenarios/host-write-cap.md#bunkerbox). The guest writes into a quota'd loopback ext4 image, so a runaway agent fills a 5 GB file rather than your disk. vivarium shares the host tree with no bound at all, and reached the same question from this reading and decided the other way: bounding a shared tree means interposing a copy-on-write layer between the user and their own directory, so vivarium bounds only the storage it creates.
- [A native command](./scenarios/native-command.md#bunkerbox). `opencode` is a symlink to the bunkerbox binary and the invoked name selects the config. You type the agent's name; vivarium makes you type `viv`.
- [Credentials sealed between runs](./scenarios/credentials-at-rest.md#bunkerbox). vivarium holds no identity by rule and so cannot offer this at all.
- [Per-tool scoping for free](./scenarios/per-tool-credentials.md#bunkerbox), because the tool is the unit the whole design is organized around.
- A journaled session home that survives a crash, and five agents packaged and running today.

## What vivarium does better

- [No build leaves the boundary](./scenarios/build-inside-boundary.md#vivarium), and nothing has to be configured for that to be true.
- [Composition](./scenarios/composition.md#vivarium) through the NixOS module system, with [collisions reported](./scenarios/collision.md#vivarium) rather than resolved by union.
- [The same definition reproduces](./scenarios/same-definition.md#vivarium), from a manifest and a lock rather than a tag over `apk add`.
- [A gating resolver](./scenarios/allowlist.md#vivarium) that installs address policy before releasing a DNS answer, where bunkerbox resolves its allowlist to addresses once at start and never again.
- [Key material never enters the guest](./scenarios/key-material.md#vivarium) — `ssh` and `gpg` relay over a credential port.
- [Re-entry and concurrent sessions](./scenarios/later-command.md#vivarium); bunkerbox's container is `--rm` and cannot be rejoined.
- Per-user operation. vivarium needs no `sudo`; bunkerbox goes through it for `ctr`, `iptables`, `mount`, and `systemctl` alike.

## The practical read

bunkerbox is the pragmatic answer for a team that has containerd, an Ubuntu fleet, and agents to package this quarter. It accepts a hole — host-side builds, behind bubblewrap when you configure it — in exchange for working with the world as it already is.

vivarium is the strict answer and pays for strictness with a hard prerequisite and a longer road. bunkerbox's passthrough is the strongest argument in this set for not choosing it, and vivarium's response is already recorded rather than improvised: the inner layer provisions its own store, which makes reaching for the host unnecessary rather than merely forbidden.

Two things this comparison surfaced that vivarium has no answer to, and both are recorded rather than open:

- Credentials encrypted at rest between runs, which the [no-decryption-identity rule](../spec/08-invariants-and-guarantees.md) forecloses on purpose. The rule is defensible; that it leaves a real need to somebody else is worth saying out loud.
- A bound on what a guest writes into a shared workspace, settled against a bound at [the workspace contract](../spec/06-workspace-and-project-environment.md). What bunkerbox caps, vivarium leaves to the user's own disk.
