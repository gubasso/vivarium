# 014 — Workspace and persistence

## Goal

Give the sandbox the project tree and a home that survives a restart, so that the milestone this chain aims at is reached: a microVM a coding agent can be run inside.

## Appetite

2 implementation sessions.

## Core

The project tree is mounted read-write at the specified path and an edit made inside the guest is visible on the host, the guest home survives a `viv stop` followed by a `viv start`, and `viv destroy` removes what it owns so the next `viv start` is a cold rebuild.

## In scope

Ordered, because a workspace the guest cannot write to makes every persistence question untestable.

1. Mount the project tree read-write at the specified path and confirm an edit crosses the boundary in both directions.
2. Settle whether the share identity translation and the host link-farm masking are correctness requirements for this mount rather than robustness polish. If either is, it belongs in [slice 012](../012-first-boot/README.md)'s `Governed by` and is recorded there; the answer is owed before item 3.
3. Back the guest home with a volume that survives stop and start.
4. Implement `viv volume` list and the prune boundary the volume model fixes.
5. Implement `viv destroy` and its cold-rebuild boundary.
6. Land the three `Virtualization` trials this slice's `Acceptance` names, unskipped, and restore the pre-push gate as the last of them goes green. `profile.pre-push` selects `kind(test) - binary(user_workflows)` because the acceptance trials assert verbs slices 013 and 014 implement; this slice is where that whole-binary exclusion narrows to the one trial still waiting on a verb, `- test(=workflow_05_restrict_egress_allowlist)`, which [slice 004](../004-enforce-egress-allowlist/README.md) removes when it enforces the allowlist. Narrowing rather than deleting, because the binary carries trials this slice greens and one it cannot: dropping the clause outright would put the stage straight back to failing on a capable host. [`../../sequencing.md`](../../sequencing.md) owns the obligation and carries the measurement to replace.

## Out of scope

- Store reclamation under space pressure, and any collection behavior beyond what `viv destroy` removes.
- Generations and `viv update`.
- Secrets and config sharing beyond the credential relay already proved.
- Extra mounts past the project tree and the home. The manifest may declare them; exercising more than the core needs is remainder.
- Ordered remainder, cut first when the appetite binds: `viv gc` store sweeping, `viv trim`, and volume format migration.

## Governed by

- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — defines the workspace mount and its confinement.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — defines the stop, start, and destroy boundaries.
- [`../../../explanation/guest-store-and-volumes.md`](../../../explanation/guest-store-and-volumes.md) — owns the volume topology.
- [`../../../explanation/shared-filesystems.md`](../../../explanation/shared-filesystems.md) — owns the share behavior item 1 depends on.
- [`../../../decisions/ADR-0019-volume-model.md`](../../../decisions/ADR-0019-volume-model.md) — fixes the volume model.
- [`../../../decisions/ADR-0037-volume-disk-format-and-reclamation.md`](../../../decisions/ADR-0037-volume-disk-format-and-reclamation.md) — fixes the disk format and reclamation boundary.
- [`../../../decisions/ADR-0043-identity-marker-lifecycle.md`](../../../decisions/ADR-0043-identity-marker-lifecycle.md) — fixes what `viv destroy` does to the marker.
- [`../../../decisions/ADR-0066-share-uid-gid-translation.md`](../../../decisions/ADR-0066-share-uid-gid-translation.md) — fixes identity translation across the share; item 2 settled it as first-boot correctness, so [slice 012](../012-first-boot/README.md) now carries it too.
- [`../../../decisions/ADR-0067-volume-prune-and-first-boot-home.md`](../../../decisions/ADR-0067-volume-prune-and-first-boot-home.md) — fixes prune and the first-boot home.
- [`../../../decisions/ADR-0080-the-sandbox-is-disposable.md`](../../../decisions/ADR-0080-the-sandbox-is-disposable.md) — fixes what may and may not be treated as a system of record.
- [`../../../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md`](../../../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md) — fixes the masking; item 2 settled it as first-boot correctness, so [slice 012](../012-first-boot/README.md) now carries it.
- [`../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md`](../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md) — fixes where item 1 mounts the project tree, and supersedes ADR-0017.

## Acceptance

When the guest writes to the workspace mount, the host SHALL observe the change at the project tree, and the reverse SHALL hold.

When a sandbox is stopped and started again, its home volume SHALL retain what was written before the stop and `workflow_07_stop_restart_preserving_volumes` SHALL pass unskipped.

When `viv stop` is issued, shutdown SHALL complete inside the specified budget and `workflow_07_stop_completes_within_budget` SHALL pass unskipped.

When `viv destroy` completes, the next `viv start` SHALL be a cold rebuild and `workflow_08_destroy_cold_rebuild` SHALL pass unskipped.

If item 2 finds that identity translation or link-farm masking is a correctness requirement for first boot, then that decision SHALL be recorded in slice 012's `Governed by` and in this slice's `Revisions`.

## Rabbit holes

- Treating guest durability as a system of record — escape: the sandbox is disposable, so persistence here means a home that survives a restart, not data the user may rely on.
- Chasing store reclamation because a volume raised the question — escape: reclamation under pressure is a decided but unfunded cluster; record the observation and leave it there.
- Answering item 2 by moving the ADRs without evidence — escape: the answer is whether first boot is wrong without them, and that is observable from slice 012's booted guest.
- Growing the mount surface to whatever the manifest can declare — escape: the project tree and the home are the core; anything else is remainder.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, a coding agent has been run inside the sandbox against the mounted project tree at least twice, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved to Implemented, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Item 1 grew a decision, and the appetite did not move to pay for it. "The specified path" was `/workspaces/<repo>` when this slice was shaped, and two things were found on reaching for it. The first is a defect: [`../../../../nix/guest.nix`](../../../../nix/guest.nix) mounted the literal `/workspaces/vivarium`, correct only for a project directory named `vivarium` and unnoticed because no trial had ever written across the boundary or compared the path against the project name. The second is that the specified path was itself wrong. Git's linked worktrees record two absolute paths, and `git worktree add` takes the back-pointer from the repository it resolved through the current directory, which no argument overrides — so a tree reachable only at a guest-invented path yields worktree metadata valid on one side, failing quietly enough that `git worktree list` reports it healthy from both. The workspace is therefore mounted at the host's own absolute path, decided in [`../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md`](../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md), which supersedes ADR-0017 by choosing the option that record considered and rejected, and amends N16.

The mechanism was forced rather than chosen. A share's mount point is build output and N19 keeps a host path out of one, so the share mounts at a build-time constant, the host path crosses on the kernel command line, and a guest unit binds it into place before the agent accepts a session. That constant is not under `/run/vivarium`: the agent declares `RuntimeDirectory=vivarium`, so systemd would delete the directory holding a live mount whenever that unit restarted.

Item 2 is answered, and both ADRs move. [`ADR-0092`](../../../decisions/ADR-0092-the-guest-masks-the-host-link-farm.md) needed no new measurement: its own record is a first boot that failed without the mask, with the guest exhausting the share daemon's descriptor budget before one collection finished. [`ADR-0066`](../../../decisions/ADR-0066-share-uid-gid-translation.md) is correctness too, but the evidence is not the obvious one and the difference is worth stating, because the obvious one passes vacuously. On a host whose user is uid 1000 the map half coincides exactly with the daemon's own identity default, so a workspace round trip on such a host separates nothing — it would pass with the translation removed. What does separate them is the half that is not a default: the forbid and squash ranges, which slice 012's own legs met at first boot. Both the diagnostic's inner `nix develop` and the collection-interlock handshake had to be written to run as the mapped user, because guest root falls inside `forbid-guest` and every workspace syscall it makes is refused. That is first boot being wrong without the decision, observed from slice 012's booted guest, which is what this item asked for.

A trial now carries the other half. `workflow_09_workspace_round_trip` reads a guest-written file's owner from the host side, which is the one place the translation is visible as an outcome rather than as an argument, and it is also the first trial to assert the workspace contract at all — the slice's first acceptance clause had named no trial.

Item 3 found a defect where it expected plumbing, and the defect was `viv stop` rather than the volume. The home volume is retained, reattached, and mounted from the same image across a restart — all of which measured clean — and a file written into it was still gone afterwards. What separated the two was a `sync`: an explicitly synced file survived and an ordinary one did not, which places the loss in the shutdown path and nowhere near the volume. No volume assertion could have found it, and the trial that will name this behaviour writes without syncing, so it would have failed for a reason nobody could have read off the volume.

Two causes, both fixed. The transient unit declared `KillMode=control-group`, so `systemctl stop` delivered the stop signal to every process at once and cloud-hypervisor died where it stood — the guest never ran its shutdown transaction and never unmounted. It is now `KillMode=mixed`, which signals the supervisor alone and still kills whatever remains at the stop timeout, so nothing is given up. And the supervisor's own teardown went straight to `ch-remote shutdown`, which destroys a VM rather than powering it down; it now walks spec/10's ladder, raising the ACPI power button first and falling through to the destroy only when the guest does not take it. Measured after: an unsynced write survives a stop and start, twice, and `viv stop` completes in about 1.6 seconds against the ten-second grace spec/10 gives it.

Item 3's other half was a race that would have passed most boots. [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) requires the first-boot home ownership applied before the agent accepts a session, and `vivarium-volume-prepare` and `vivarium-agent` were both `wantedBy = multi-user.target` with no edge between them — confirmed by reading `systemctl show -p After vivarium-agent.service` inside a live guest, where neither name appeared. The agent now requires and follows it, and [`../../../../tests/nix/contract.sh`](../../../../tests/nix/contract.sh) asserts the edge on the built unit rather than leaving it to a boot that would usually win the race anyway. The same unit gained the skeleton seeding spec/06 asks for; the shipped image carries no `/etc/skel`, so it copies nothing today, which is stated in the unit rather than left to be discovered.

Not implemented, and named rather than implied. `[volume].persist` is still inert: the option is typed and emitted and nothing binds those paths out of the home volume. The behavioural proof of the durability fix is also still owed — the guards that landed here are `KillMode=control-group` being forbidden by name in a unit test and the ordering edge asserted in the build contract, while the trial that writes without syncing and restarts is `workflow_07_stop_restart_preserving_volumes`, which item 6 lands once `viv volume list` exists. And the repair reaches the default grace only: `viv stop --timeout` is honoured by the host's own deadline and does not reach the supervisor that owns the graceful rung, so asking for longer does not give a slow guest longer to commit — [`Q-018`](../../open-questions.md), raised where the ladder was rewritten rather than absorbed into it.

Two limitations are recorded rather than absorbed, because both are things a user meets and neither is fixed here. A bound linked worktree still cannot reach its main repository, whose git directory lies outside the one shared tree — [`Q-016`](../../open-questions.md), whose exit is the declared-mount surface this slice's `Out of scope` already excludes. And a host user named `vivarium` cannot be mirrored at all, because the guest home occupies that path — [`Q-017`](../../open-questions.md).

Items 4 and 5 landed, and item 4 grew a dependency the shaping did not see. `viv volume list` reports a `cache` volume and `workflow_07_stop_restart_preserving_volumes` asserts an image for it, so the trial could not go green while a manifest's `[[volumes]]` stayed declarable and inert — which it had been since the option was typed. Attaching a declared volume is a build-channel change, so it reached [`../../../../nix/guest.nix`](../../../../nix/guest.nix), the launch contract, and the runner's own argv. Named rather than absorbed, because `Out of scope` excludes extra mounts past the project tree and the home, and this is the one place the exclusion did not hold: the trial was already written, spec/06 already specified the feature, and narrowing the trial would have been the alternative.

The mechanism was chosen against one that does not work. The launcher used to take `--volume` and `--store-volume`, one image path per role, which cannot express a number of volumes decided by a manifest. Passing a repeatable `--volume name=path` was the obvious replacement and is not available: `viv start --no-rebuild` evaluates nothing, so on that path the host does not know which volumes the build declared. The launcher does — it reads the build's own contract — so it now takes one `--volume-dir` and joins each image path from a name the build owns. That moved the `.img` naming out of Rust and next to the guest that mounts them, removing a double spelling rather than adding one, and it bumped the launch schema to 5.

Two constraints were found rather than designed. ext4 caps a volume label at 16 bytes and `mkfs.ext4 -L` truncates past it while exiting `0`, which would leave a guest waiting on a `/dev/disk/by-label` node that never appears — so a declared volume's label is `viv-<name>`, the two reserved labels are unchanged because renaming one orphans every image already on disk, and the budget is asserted at evaluation rather than assumed. And upstream `microvm.nix` requires each volume's `image` to be unique, which is why the sentinel became a directory rather than staying a per-role constant.

Item 4's `declared_by` column forced a decision the shaping did not anticipate either. Provenance exists only inside a merged evaluation, and spec/14 gives `volume list` and `volume prune` one failure column each while an evaluation has four — so neither verb may evaluate. Reading the manifest's own `[[volumes]]` instead was considered and refused: a volume declared by a piece would then be invisible, classified as an orphan, and deleted by `prune`. The answer is a per-target record the evaluating command writes, which spec/01 already required in so many words for `prune`'s `mount` column. Its freshness rule, and the one-start lag it leaves for a piece that edits its own volumes, are recorded in [`../../../explanation/guest-store-and-volumes.md`](../../../explanation/guest-store-and-volumes.md) rather than here, because they are a property of the topology and not of this slice.

`viv volume prune` shipped alongside `list` rather than being cut, which widened item 4 past what its wording requires — "the prune boundary" could have meant the orphan predicate alone. It shares one enumeration with `list`, which is what ADR-0067 asks for, and the reserved exclusion is held by the order that enumeration appends its three arms rather than by a name test that could drift. No acceptance trial names it; its predicate is unit-tested.

`viv destroy` needed the first interactive prompt in the tree. spec/01 and spec/10 both say it prompts on a TTY, and `viv init --write` had set the opposite precedent by refusing instead. The prompt is a pure function over a reader and a writer, so the answer table is testable without a terminal — the acceptance harness pipes stdin and therefore exercises only the `--yes` path. Destroy also needed the first removal in [`../../../../src/config/identity.rs`](../../../../src/config/identity.rs), which had `mint` and `resolve` and nothing that took anything away. Two details there are load-bearing: the index row is dropped by path and never by id, because a row holding this id at another path belongs to a copy that disambiguated against this project; and the marker's two files are unlinked individually rather than with `remove_dir_all`, so a `.vivarium/` a user put something of their own into survives as a non-empty directory.

Item 6 is done and the slice is not closed. All three acceptance trials pass unskipped, `profile.pre-push` subtracts exactly `- test(=workflow_05_restrict_egress_allowlist)` and runs 26 of 26 twice, and [`../../sequencing.md`](../../sequencing.md) carries the replacement measurement its own step 3 asked for. What `Done when` still owes is the coding-agent run: an agent has not been run inside the sandbox against the mounted project tree, twice or at all, so [`milestones.md`](../../milestones.md) keeps this row `active`.

One thing is smaller than it reads. `destroy` unlinks the project's build records and its state subtree, and spec/10 also has it unlink every generation GC root — but spec/11's per-project Nix profile does not exist yet, so that step is structurally a no-op today. The cold rebuild the trial observes comes from removing the build records, and [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) says so in the row rather than leaving it to be discovered.
