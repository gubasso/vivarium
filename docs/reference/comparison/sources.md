# Sources

What every verdict in [`README.md`](./README.md) rests on. Each subject is pinned to a commit, a release, or a dated document, so a later reader can tell whether a cell has gone stale without re-deriving it.

## Subjects

### `vivarium`

Verified: 2026-08-18 — this repository, branch `initial-implementation` at `ceb0027`.

| What it establishes                                                   | Where                                                                                                            |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| The binding product rules, named in words throughout                  | [`docs/reference/spec/08-invariants-and-guarantees.md`](../spec/08-invariants-and-guarantees.md)                 |
| Which commands run and which are specified only                       | [`docs/reference/implementation-status.md`](../implementation-status.md)                                         |
| The lifecycle ladder and what `stop` reaches                          | [`docs/reference/spec/10-vm-lifecycle.md`](../spec/10-vm-lifecycle.md)                                           |
| Generations, rollback, and reclamation                                | [`docs/reference/spec/11-generations-and-build-history.md`](../spec/11-generations-and-build-history.md)         |
| Egress modes and the gating resolver                                  | [`docs/reference/spec/05-networking-and-egress.md`](../spec/05-networking-and-egress.md)                         |
| The artifact model, and how a shared piece declares its own inputs    | [`docs/reference/spec/03-artifact-model.md`](../spec/03-artifact-model.md)                                       |
| Module merge, the priority convention, and the one effective lockfile | [`docs/reference/spec/04-composition-and-determinism.md`](../spec/04-composition-and-determinism.md)             |
| Mount sources, the agent channel, and what never crosses              | [`docs/reference/spec/07-secrets-and-config-sharing.md`](../spec/07-secrets-and-config-sharing.md)               |
| The two-layer design, and what the guest base must ship for it        | [`docs/reference/spec/06-workspace-and-project-environment.md`](../spec/06-workspace-and-project-environment.md) |
| Unresolved scope questions the tables point at                        | [`docs/plan/open-questions.md`](../../plan/open-questions.md)                                                    |

Rules cited by name, and what each fixes:

| Name used here                       | What it fixes                                                                              |
| ------------------------------------ | ------------------------------------------------------------------------------------------ |
| separate-kernel rule                 | A hardware-virtualization boundary with its own guest kernel, and no shared-kernel mode    |
| class-not-tool rule                  | The boundary is a capability class; no named hypervisor is part of the contract            |
| pure-build rule                      | The build takes no host-specific input: same manifest closure and lock, same output        |
| NixOS-module composition rule        | Layers merge through the NixOS module system, not a bespoke engine                         |
| one-manifest-per-project rule        | Exactly one manifest resolves per project, failing closed when none applies                |
| egress-defaults-open rule            | Egress is unrestricted by default, with one knob for a default-deny allowlist              |
| never-touch-user-files rule          | vivarium does not modify a project's own user-authored files                               |
| no-secrets-in-the-store rule         | A secret never enters the build or the store; runtime-injected or encrypted-at-rest only   |
| personal-data-free-artifact rule     | A shared image or piece carries no literal personal path or plaintext secret               |
| host-symmetric mount rule            | The workspace mounts inside the guest at its host absolute path                            |
| deny-by-default environment rule     | Host environment crosses only by allowlist or explicit `--env`                             |
| non-destructive stop rule            | `viv stop` removes no volume, generation, or state                                         |
| sandboxed-VMM rule                   | The monitor and every host-side helper run under a seccomp and capability sandbox          |
| marker-anchored identity rule        | Project identity is the marker-anchored directory name, surviving a rename                 |
| ceilings-not-reservations rule       | Declared resources bound use rather than reserving it                                      |
| pre-launch capacity check            | `viv start` refuses when the host cannot serve the free-memory reserve                     |
| session-directories-never-cross rule | No mount source resolves to a host session directory or an ancestor                        |
| no-decryption-identity rule          | vivarium decrypts nothing and holds no identity                                            |
| agent-channel allowlist              | Only `ssh` and `gpg` forward, over a credential port, with no host path in the declaration |

### `flake-pilot`

Verified: 2026-08-18 — <https://github.com/OSInside/flake-pilot> at `main`. Rust, MIT licensed. Partially re-read 2026-08-20 against a fresh clone at `44e3ab2`: the boundary, engine-selection, and firecracker-networking rows, which are the ones that had rested on a non-public source, and later the same day the `Guest environment` rows added for packages, the inner environment, and setup hooks. The rest of this subject still carries its 2026-08-18 reading.

| What it establishes                                                            | Where                                                                                                                |
| ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| Two pilots, the registration model, and the symlink launcher                   | The upstream `README.md`                                                                                             |
| The container schema, including the all-or-nothing `--opt` rule                | `podman-pilot/src/config.rs`                                                                                         |
| The firecracker schema, `overlay_size`, and `force_vsock`                      | `firecracker-pilot/src/config.rs`                                                                                    |
| Registration flags and call-time `@` and `%` pseudo-arguments                  | The manual-page sources under `doc/`                                                                                 |
| `sci` as guest init: run one command, then reboot                              | The `sci` manual page                                                                                                |
| That an image is built outside flake-pilot, by whatever tooling the user likes | The upstream `README.md`                                                                                             |
| The three registrations for one AI tool, and what each rung costs              | The upstream `README.md`, sections "Register claude AI as podman app" and "Register claude AI as firecracker VM app" |
| `krun` as the deeper-isolation runtime, and why it loses `exec` and `--resume` | The upstream `README.md` note beginning "For deeper isolation based on a VM"                                         |
| Firecracker networking as the user's own responsibility                        | The upstream `README.md`, "Firecracker Networking"                                                                   |

Every row above is public and linkable at <https://github.com/OSInside/flake-pilot>. An earlier draft of this page also cited a conference deck on the same project, read 2026-08-18; it has been removed rather than reworded. Two reasons. The deck has no public location, so a reader could not check a cell against it — and the one fact this document had taken from it and from nowhere else, an engine list naming `kata` alongside `crun` and `krun`, does not survive contact with the source: `kata` appears nowhere in the flake-pilot repository at `main`. What the repository does establish is narrower and is what the tables now say — the OCI runtime is podman's own selection, passed through as `--opt "\--runtime=krun"` or set as `runtime = "krun"` in `containers.conf`, so the reachable set is whatever podman accepts rather than a list flake-pilot publishes.

### `glaipnir`

Verified: 2026-08-18 — <https://github.com/val4oss/ai-agents-sandbox> at commit `21ef389`. POSIX shell, AGPL-3.0. The tool is `glaipnir`; its images and containers keep the older name `ai-agents-sandbox`. Partially re-read 2026-08-20 against a fresh clone at the same commit: the image, its hooks, and its package mechanism only, for the three `Guest environment` rows added that day. Every other row still carries its 2026-08-18 reading.

| What it establishes                                                  | Where                         |
| -------------------------------------------------------------------- | ----------------------------- |
| The whole CLI, and the microVM capability probe                      | `glaipnir.sh`                 |
| Per-agent credential mounts, and which agent sees which directory    | `_bind_agent_mounts`          |
| Egress bound to a non-VPN interface, and the abort when none exists  | `_detect_public_iface`        |
| Two agents forced out of microVM mode by `containers/libkrun#674`    | `run()`                       |
| The image, its labels, and the entrypoint's privilege drop           | `image/`                      |
| Build and run hooks, and the `PACKAGES` array                        | `docs/overview.md`            |
| Run hooks executed in sorted order on every start                    | `image/scripts/entrypoint.sh` |
| The fixed openSUSE base, carrying no Nix, direnv, or version manager | `image/Containerfile`         |
| The macOS VPN enforcer, and what it costs                            | `scripts/`, `launchd/`        |

### `podman`

Verified: 2026-08-19 — plain rootless podman 5.x with no wrapper, read at `podman run --runtime krun` per the methodology in [`README.md`](./README.md). Prerequisites at that setup: libkrun installed and `/dev/kvm` accessible.

| What it establishes                                                                                  | Where                                                                                                                                      |
| ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `--runtime` selecting an OCI runtime, `-v`, `--env`, `--network`, `-p`, `exec`, `ps`, `prune`        | The upstream manual pages, <https://docs.podman.io>                                                                                        |
| Secret drivers, and which of them encrypts at rest                                                   | The `podman-secret-create` manual page, <https://docs.podman.io/en/latest/markdown/podman-secret-create.1.html>                            |
| `Volume=` in a Quadlet unit, and that it matches `--volume`                                          | The `podman-systemd.unit` manual page, <https://docs.podman.io/en/latest/markdown/podman-systemd.unit.5.html>                              |
| libkrun as a microVM runtime, the libkrunfw guest kernel, TSI networking                             | The libkrun project README, <https://github.com/containers/libkrun>                                                                        |
| That impersonation carries connections inbound to a listening guest port, and what it does not carry | The networking section of that README                                                                                                      |
| The annotation that swaps impersonation for a virtio-net interface                                   | The `krun` manual page in `crun`, <https://github.com/containers/crun/blob/main/krun.1.md>                                                 |
| An `AF_INET6` listener reported as not forwarded, unsettled against the README                       | podman issue `25494`, <https://github.com/containers/podman/issues/25494>                                                                  |
| A published port reached from the host under `krun`                                                  | José Castillo Lema, "Playing with Podman crun backends: Wasm(Edge) and libkrun", <https://josecastillolema.github.io/podman-wasm-libkrun/> |

It is in the tables because it is the baseline most readers arrive from, and because its `crun` default is the honest name for what `flake-pilot`'s first isolation level runs on.

## Re-verification

Verified: 2026-08-18 — cadence set against the release rhythm of each subject named above.

| Subject       | Cadence                 | What moves                                                                 |
| ------------- | ----------------------- | -------------------------------------------------------------------------- |
| `vivarium`    | Every slice that closes | `implementation-status.md` is what decides a `*`                           |
| `flake-pilot` | 90 days                 | A registration schema key, or a new engine                                 |
| `glaipnir`    | 30 days                 | A rename, an RPM, and a 1.0.0 release all land inside one changelog window |
| `podman`      | 180 days                | Rootless defaults change slowly                                            |

The cadence above is the reading; [`tracking.yaml`](../tracking.yaml) is what schedules it, and that file's `last_checked` date for this set is the one a reader should trust when the two disagree. The table stays here because it names what moves for each subject, which a `revalidate` line cannot carry per-subject.

A refresh re-reads the material at a new commit and updates the `Verified:` line above each table. Editing a version number without re-reading produces a false date, which is worse than no refresh because it resets the reader's suspicion.
