# 020 — Many workspaces in one sandbox

## Goal

Let one sandbox hold several project trees, each mirrored at its own host path, so a user who keeps a family of related repositories runs one VM for all of them instead of one VM each.

## Appetite

3 implementation sessions.

## Core

A manifest declaring several project trees boots one VM with every one of them mirrored at its own host path, `viv shell` and `viv exec` from inside any declared tree start in that tree, and a declared set that collides with a guest-owned path or nests within itself is refused before boot.

## In scope

Ordered, because the declaration decides everything the launch and session halves then read.

1. Enact ADR-0108's `[[workspaces]]` table: a workspace declares a host `source` and no `target`, because it mirrors its host path. It reaches the manifest parser and the typed piece options ([`../../../reference/spec/03-artifact-model.md`](../../../reference/spec/03-artifact-model.md)) alongside the `target`-mounted kind ADR-0020 owns, which stays legal and unchanged.
2. Generalize N16's host-symmetric mirroring from the one primary workspace to every declared workspace, and run ADR-0100's refusal set pairwise as ADR-0108 requires: no declared workspace may equal, contain, or lie under a path the guest owns, and no two may nest in each other.
3. Replace the single privileged share tag. [`../../../../src/launch/spec.rs`](../../../../src/launch/spec.rs) defines one `WORKSPACE_SHARE_TAG` whose share the boot record names and whose mount point a session starts in; the boot record grows to carry the set, and the session's starting directory is chosen by matching the invoking host directory against it.
4. Enact ADR-0109's refusal in every command that resolves a manifest, as one routine rather than a check per verb: a working directory inside no declared workspace exits `78`, and the message names the resolved manifest, the undeclared directory, and the block to add. vivarium writes nothing.
5. Land the acceptance trial: two project trees declared in one manifest, one VM, a round trip through each, and a session started from each landing in the right tree.
6. Move the rows this slice changes in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), and update the guide surface that currently describes one project tree per sandbox.

## Out of scope

- Sandbox identity, the registry, and the project-to-manifest binding. A sandbox is still keyed by the project directory here; [slice 021](../021-the-manifest-is-the-sandbox/README.md) is what changes that.
- Attaching a tree to a sandbox that is already running. Every workspace is established at launch, as N15 requires.
- Any per-invocation source expansion. A declared workspace resolves to one value per user, which is what keeps the set a property of the configuration rather than of the call.
- Ordered remainder, cut first when the appetite binds: item 6's guide sweep, and the trial leg that exercises more than two trees.

## Governed by

- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — defines the workspace mount and what its share guarantees.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — defines the session's working directory and the ensure-running protocol items 3 and 4 change.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N16, whose scope item 2 widens.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — draws the `65`/`78` boundary item 4's refusal sits on.
- [`../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) — fixes the declaration surface item 1 enacts and widens N16 for item 2.
- [`../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md`](../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md) — fixes the refusal and the message contract item 4 enacts.
- [`../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md`](../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md) — fixes the mirroring and the refusal set item 2 generalizes.
- [`../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes the declaration schema ADR-0108 extends rather than replaces.
- [`../../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../../decisions/ADR-0009-launch-time-workspace-path-injection.md) — fixes why a workspace path is injected at launch and never built in.

## Acceptance

When a manifest declares several project trees, `viv start` SHALL boot one VM in which each tree is readable and writable at its own host path, and a trial SHALL round-trip an edit through each.

When `viv shell` is invoked from inside a declared tree, the session SHALL start in that tree.

If a declared workspace equals, contains, or lies under a path the guest owns, then the launch SHALL refuse before boot.

If two declared workspaces nest in each other, then the launch SHALL refuse before boot and the message SHALL name both.

If a command that resolves a manifest is invoked from a directory that manifest declares as no workspace, then it SHALL exit `78` before any build or boot, and the message SHALL name the resolved manifest, the undeclared directory, and the block that declares it. A trial SHALL assert the code, the three named parts, and that the manifest is unchanged afterwards.

## Rabbit holes

- Reaching for `${PWD}` or any per-invocation expansion to spare the user a declaration — escape: a name that takes more than one value for one user against one configuration forces identity to include that name, which is the thing this chain exists to avoid.
- Making one declared tree secretly primary so the old singleton survives — escape: every declared workspace is equal; the session's starting directory is derived from the invoking directory, not from a privileged entry.
- Growing the refusal set into a general path-overlap solver — escape: the checks are the ones ADR-0100 already names, applied pairwise; nothing more.
- Rewriting the ensure-running protocol because the boot record changed shape — escape: the protocol is unchanged, the record carries a set where it carried one value.
- Writing the missing declaration into the user's manifest so the command can proceed — escape: N9 and N13 forbid the side effect, and ADR-0109 makes the message the deliverable; the user applies the edit.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, `ADR-0108` and `ADR-0109` carry this slice as their enactment, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-18, at shaping, before any work started. This slice is where the original request is actually served: after it, several projects share one VM, declared explicitly in one manifest. [Slice 021](../021-the-manifest-is-the-sandbox/README.md) makes that arrangement ergonomic and cheaper to key, but a user who wants the shared sandbox has it once this slice closes.

Item 2 is the load-bearing one and the reason it precedes the session work. The shaping considered leaving extra trees at a declared `target` and mirroring only a primary, which is simpler and was rejected: [`ADR-0100`](../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md) adopted mirroring because git's linked worktrees record two absolute paths and a tree reachable only at a guest-invented path yields metadata that resolves on one side while `git worktree list` reports it healthy from both. That failure mode does not care which tree is primary, so mirroring some trees and not others would ship the quiet breakage ADR-0100 exists to prevent, for every tree after the first.

Item 4 exists because the current answer is structural rather than chosen. A session starts in the one workspace share's mount point because there is exactly one, so no rule was ever needed; with a set, the absence of a rule becomes a behavior nobody decided. [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) already records a neighbouring rough edge — a subdirectory of the workspace starts at the workspace root — which is the same seam and should be settled with it rather than around it.

This slice depends on [slice 019](../019-declared-mounts-reach-the-guest/README.md) and cannot start before it. Declared mounts do not reach the guest today, so there is no share list to grow and no trial here could distinguish a mirroring defect from the inertness underneath it.

Reshaped 2026-08-20, before any work started, so `Goal`, `Core`, `Appetite`, and `Acceptance` moved while the slice was still shaped. Items 1 and 4 stopped being decisions and became enactments: [`ADR-0108`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) settles the declaration surface as its own `[[workspaces]]` table, and [`ADR-0109`](../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md) settles what a command does outside one. Item 4's acceptance clause gained the shape it had deferred.

The reason the table beat a flag on `[[mounts]]` is worth keeping here rather than only in the decision: a workspace mirrors its host path, so it has no `target` to declare, and a row whose distinguishing feature is a missing field is a weaker contract than a row in a table that means one thing. The reason the refusal beat a default is the same argument item 4 already carried — a default makes one command mean different things in different directories.

Started 2026-08-21 on the capable host, and the host premises the shaping could not confirm were confirmed before the first command: `/dev/kvm` present, a systemd user manager `running`, `$XDG_RUNTIME_DIR` set, and `VIVARIUM_HEAVY_DRIVE` naming a directory under an attached drive. Every build, evaluation, and trial in this slice ran through [`tests/host/heavy-run`](../../../../tests/host/heavy-run).

Items 1 and 2 landed together as one checkpoint. Three things moved that the shaping did not name. The fixture sweep item 3 anticipated was pulled forward into this checkpoint, because with workspaces explicit every booting fixture refuses until it declares one, so the Rust gate could not go green without it. `workspacesInternalRoot` was declared in [`nix/default.nix`](../../../../nix/default.nix) rather than in the guest module, so that [`tests/nix`](../../../../tests/nix) inherits the same value it asserts against — a check whose two sides are both literals in the checking file reports on nothing. And the strongest rendered-specification assertion in [`tests/host/exec-and-shell-check`](../../../../tests/host/exec-and-shell-check) moved from the shipped image to the measurement image, because the shipped image declares no workspace and the runner refuses a `--workspace` argument against a contract carrying no `ws` tag. Whether the shipped package deserves a zero-workspace rendering lane of its own is open, and recorded at that call site rather than settled here.

N24 stays scoped to `[[mounts]]`, decided rather than inherited. A workspace mirrors its own host path, so it must pass `unmirrorable` against `GUEST_OWNED_PATHS`, and that list already holds `/tmp`, `/var/tmp`, and `/run/user`. Running the mount rule over workspaces too would refuse the same paths a second time in different words, and would make whether a workspace resolves depend on the host's `${XDG_RUNTIME_DIR}` — which, since a gated run moves `TMPDIR` onto the drive and a bare run leaves it at `/tmp`, would make the acceptance fixtures' own resolvability a property of how the suite was invoked. One hazard, one rule, at the surface that owns it. `/run/vivarium-mounts` joined `GUEST_OWNED_PATHS` in the same change: the guest script had always refused it and the host list had not, so a workspace declared there failed at boot instead of at resolution. A unit test now pins the two lists against each other.

Two things are deferred to the checkpoint that lands items 3 and 4, where the resolution routine and the anchor move and they are the same question in a different place. The reuse predicate at `reusable()` still asks whether the invoking project directory is among the recorded workspace paths, which was an identity while there was exactly one workspace and it was the project tree; a manifest whose declared workspaces exclude the bound project directory therefore refuses a live sandbox as foreign and sweeps a dead record as stale. It is invisible to the suite, because the fixture sweep makes the project directory `ws0` everywhere. Deciding it needs the anchor, and deciding it must not cost the property that a reused VM evaluates nothing (spec/12 step 3). The second is the decidable tier: [`src/config/merged.rs`](../../../../src/config/merged.rs) still walks only `mounts`, so a workspace source defect that the manifest text alone could decide reaches the launch tier instead of the merged view.
