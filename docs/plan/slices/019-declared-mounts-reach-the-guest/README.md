# 019 — Declared mounts reach the guest

## Goal

Make a mount a manifest or a piece declares actually mount, so the declared-mount surface stops being typed, emitted, and inert — and the two slices after this one have a share list they can grow.

## Appetite

3 implementation sessions.

## Core

A `[[mounts]]` entry declared by a manifest or by a piece is mounted in the running guest at its declared `target` with its `readonly` flag honoured, served by its own confined daemon, and a `source` that is unset, missing, not a regular file or directory, or a host session directory is refused before boot rather than after it.

## In scope

Ordered, because nothing downstream can be observed until a declared mount reaches the guest at all.

1. Derive the guest's share list from `config.vivarium.mounts` instead of the hardcoded pair in [`../../../../nix/guest.nix`](../../../../nix/guest.nix), and follow that derivation in [`../../../../nix/launch-arguments.nix`](../../../../nix/launch-arguments.nix). These two files each hardcode exactly two shares today, which is the whole reason the surface is inert.
2. Construct one `ShareSpec` per declared mount on the host side. [`../../../../src/launch/spec.rs`](../../../../src/launch/spec.rs) already carries a share list and [`../../../../src/launch/supervisor.rs`](../../../../src/launch/supervisor.rs) already spawns one confined daemon per entry; no construction site outside tests builds a share from a mount.
3. Land the launch-time validation ADR-0020 requires — an unset variable or a missing host path aborts before boot with a legible error — together with ADR-0071's amendment that a `source` is a regular file or a directory and never a socket, FIFO, or device node, and N24's session-directory refusal at `78`. This is the answer [`Q-012`](../../open-questions.md) asks for, and this slice is where that question stops being theoretical.
4. Extend the confinement profile's rendered validation so every new share's daemon is checked exactly as the workspace's is, with one daemon per share and only that share's host path in its view (N20).
5. Land the acceptance trial: a piece-declared mount using a portable variable is readable inside the guest at its declared target, and a read-only declaration refuses a guest write. A linked worktree reaching its main repository through a declared mount is the leg that closes [`Q-016`](../../open-questions.md).
6. Move the rows this slice changes in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), which today reads `[[mounts]]` beyond the project tree as declarable and inert.

## Out of scope

- Mirroring a declared mount at its own host path. Extra mounts land at their declared `target` here, which is what ADR-0020 specifies; [slice 020](../020-many-workspaces-in-one-sandbox/README.md) owns the mirrored kind.
- Any change to project identity, the registry, or the binding. [Slice 021](../021-the-manifest-is-the-sandbox/README.md) owns those.
- `[volume].persist`, still inert and not this slice's surface.
- Hot-plugging a share into a running guest. Every share here is established at launch.
- Ordered remainder, cut first when the appetite binds: the linked-worktree trial leg of item 5, and a task guide for host-config mirroring.

## Governed by

- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — defines the mount surface and the per-share daemon confinement.
- [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md) — defines the portable-variable rule a shared piece's mount sources obey.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — defines the refusal codes item 3 returns.
- [`../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes the declaration schema and the per-side expansion this slice enacts.
- [`../../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md`](../../../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md) — fixes the piece-side typed option surface.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes what a `source` may be, and why a socket has its own channel.
- [`../../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — fixes the confinement item 4 extends.
- [`../../../decisions/ADR-0042-evaluation-time-content-defects.md`](../../../decisions/ADR-0042-evaluation-time-content-defects.md) — fixes which defects refuse at evaluation and which are rendered.

## Acceptance

When a manifest or a piece declares a mount with a portable `source`, the guest SHALL expose that host path at the declared `target`, and a trial SHALL read it from inside the guest.

When a declaration carries `readonly = true`, a guest write to that target SHALL fail and the host tree SHALL be unchanged.

If a declared `source` is unset, missing, or not a regular file or directory, then `viv start` SHALL refuse before boot with the code the exit matrix gives it, and no VM SHALL be left running.

If a declared `source` resolves to a host session directory, then the launch SHALL refuse with `78` (N24).

When several mounts are declared, each SHALL be served by its own confined daemon and the rendered profile SHALL validate for every one of them.

## Rabbit holes

- Rebuilding the share plumbing because the guest list became dynamic — escape: the host already spawns one daemon per share entry and validates the rendered profile per share; the missing edge is the construction of the entries, not the machinery that consumes them.
- Solving mirroring here because a mounted repository obviously wants it — escape: mirroring is slice 020's core and needs a decision this slice does not own; land the declared `target` ADR-0020 already specifies.
- Making N24 decidable from text when it is not — escape: Q-012 already records that a source named by a host variable resolves at launch, so the decidable spellings are a lint and the authoritative refusal is at launch.
- Growing the mount surface into volumes or `persist` because both touch guest paths — escape: a volume is a block device on the build channel and is decided elsewhere; this slice moves shares only.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, [`Q-012`](../../open-questions.md) and [`Q-016`](../../open-questions.md) carry the exits this slice gives them, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-18, at shaping, before any work started. This slice was shaped out of a design conversation about sharing one sandbox between several projects, and it exists because that conversation found a dependency underneath it: every route to a shared sandbox needs declared mounts to reach the guest first, and none of them do today.

The gap is narrower than "mounts are unimplemented", and this slice must not over-build on the assumption that it is wider. The declaration is specified in [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md), typed in [`../../../../nix/vivarium-options.nix`](../../../../nix/vivarium-options.nix), emitted by [`../../../../src/config/flake.rs`](../../../../src/config/flake.rs), merged and attributed by [`../../../../src/config/merged.rs`](../../../../src/config/merged.rs), and lint-checked for N11 by [`../../../../src/doctor/project.rs`](../../../../src/doctor/project.rs). What is missing is the consumption: two Nix files that hardcode a pair of shares, and no host-side construction of a share from a mount. [`Q-016`](../../open-questions.md) already recorded exactly this and named the same two files, which is why item 1 is stated as a derivation rather than as a design.

The confinement half is also narrower than it looks. [`../../../../src/launch/supervisor.rs`](../../../../src/launch/supervisor.rs) already iterates the spec's share list, spawns one `virtiofsd` per entry, and calls the profile's rendered validation over the whole set — so item 4 is a question of whether the existing loop holds for entries it has never seen rather than of new confinement machinery.

Item 3 is the largest unknown in the slice and the reason the appetite is three sessions rather than two. Three separate refusals meet at one code path, they belong to different invariants, and [`Q-012`](../../open-questions.md) records that one of them may not be decidable before launch at all.

Recorded 2026-08-18, at close. The whole slice landed in one merged pass under the appetite, ordered remainder included, with three shaping-time decisions taken at start and one measurement that reshaped an implementation detail.

A regular-file source is served through its parent directory — virtiofs exports trees — so whether a source is a file or a directory is a host fact that resolves only at launch, and no declared mount can be statically fstab-mounted at its target. Every declared mount therefore mounts at an internal point under `/run/vivarium-mounts` and a guest unit ([`mount-bind.sh`](../../../../nix/mount-bind.sh)) binds it at the declared target from a kernel-command-line plan, the workspace mirror's own pattern generalized.

The spec/06 descriptor-read sentence was chosen for full enactment and then bounded by a measurement: virtiofsd 1.14.0's `--sandbox namespace` re-opens its root inside the new mount namespace, where `/proc/self/fd/N` does not resolve — the same invocation under `--sandbox none` accepts it — and the N20 sandbox outranks the handoff. What ships is the closest honest form: an `O_PATH` descriptor check in the supervisor, held across every daemon's open, with the residual window recorded in spec/06 and bounded by the sandbox itself.

`Q-012` left through its first exit: `session-path` classifies the decidable spellings at evaluation (`65`, all layers), and [`src/launch/mounts.rs`](../../../../src/launch/mounts.rs) refuses at launch (`78`) what only expansion reveals. The `LaunchSpec` backstop keeps answering `64` for a hand-fed specification, on the `unmirrorable` precedent: the diagnostic tier owns the code the acceptance names, and the backstop guards a machine interface. `Q-016` left through its first exit, demonstrated by `workflow_17_linked_worktree_reaches_main`. The cuttable spec/07 portable-variable classifier (`non-portable-variable`, `65`) landed uncut, and ADR-0020's duplicate-target refusal landed in the merged view as an irreconcilable rather than a new `conflicts` kind, which is the boundary `Q-013` still owns.
