# 020 — Many workspaces in one sandbox

## Goal

Let one sandbox hold several project trees, each mirrored at its own host path, so a user who keeps a family of related repositories runs one VM for all of them instead of one VM each.

## Appetite

3 implementation sessions.

## Core

A manifest declaring several project trees boots one VM with every one of them mirrored at its own host path, `viv shell` and `viv exec` from inside any declared tree start in that tree, and a declared set that collides with a guest-owned path or nests within itself is refused before boot.

## In scope

Ordered, because the declaration decides everything the launch and session halves then read.

1. Decide and record the declaration surface for a workspace-kind mount, distinguished from the ordinary `target`-mounted kind ADR-0020 owns. A workspace mirrors its host path; a config mirror does not, and both remain legal. The decision belongs in a record this slice's `Governed by` then names.
2. Generalize N16's host-symmetric mirroring from the one primary workspace to every workspace-kind mount, and extend ADR-0100's refusal set to run pairwise: no declared workspace may equal, contain, or lie under a path the guest owns, and no two may nest in each other.
3. Replace the single privileged share tag. [`../../../../src/launch/spec.rs`](../../../../src/launch/spec.rs) defines one `WORKSPACE_SHARE_TAG` whose share the boot record names and whose mount point a session starts in; the boot record grows to carry the set, and the session's starting directory is chosen by matching the invoking host directory against it.
4. Decide and implement what `viv shell` and `viv exec` do when invoked from a directory in no declared workspace: refuse naming the declared set, or start at a default. Whichever is chosen is a contract line, not an implementation detail.
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
- [`../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md`](../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md) — fixes the mirroring and the refusal set item 2 generalizes.
- [`../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes the declaration schema item 1 extends rather than replaces.
- [`../../../decisions/ADR-0009-launch-time-workspace-path-injection.md`](../../../decisions/ADR-0009-launch-time-workspace-path-injection.md) — fixes why a workspace path is injected at launch and never built in.

## Acceptance

When a manifest declares several project trees, `viv start` SHALL boot one VM in which each tree is readable and writable at its own host path, and a trial SHALL round-trip an edit through each.

When `viv shell` is invoked from inside a declared tree, the session SHALL start in that tree.

If a declared workspace equals, contains, or lies under a path the guest owns, then the launch SHALL refuse before boot.

If two declared workspaces nest in each other, then the launch SHALL refuse before boot and the message SHALL name both.

If a session verb is invoked from a directory in no declared workspace, then the behavior item 4 decides SHALL hold, and the trial SHALL assert it.

## Rabbit holes

- Reaching for `${PWD}` or any per-invocation expansion to spare the user a declaration — escape: a name that takes more than one value for one user against one configuration forces identity to include that name, which is the thing this chain exists to avoid.
- Making one declared tree secretly primary so the old singleton survives — escape: every declared workspace is equal; the session's starting directory is derived from the invoking directory, not from a privileged entry.
- Growing the refusal set into a general path-overlap solver — escape: the checks are the ones ADR-0100 already names, applied pairwise; nothing more.
- Rewriting the ensure-running protocol because the boot record changed shape — escape: the protocol is unchanged, the record carries a set where it carried one value.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, item 1's decision is recorded and linked from `Governed by`, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-18, at shaping, before any work started. This slice is where the original request is actually served: after it, several projects share one VM, declared explicitly in one manifest. [Slice 021](../021-the-manifest-is-the-sandbox/README.md) makes that arrangement ergonomic and cheaper to key, but a user who wants the shared sandbox has it once this slice closes.

Item 2 is the load-bearing one and the reason it precedes the session work. The shaping considered leaving extra trees at a declared `target` and mirroring only a primary, which is simpler and was rejected: [`ADR-0100`](../../../decisions/ADR-0100-the-workspace-mirrors-its-host-path.md) adopted mirroring because git's linked worktrees record two absolute paths and a tree reachable only at a guest-invented path yields metadata that resolves on one side while `git worktree list` reports it healthy from both. That failure mode does not care which tree is primary, so mirroring some trees and not others would ship the quiet breakage ADR-0100 exists to prevent, for every tree after the first.

Item 4 exists because the current answer is structural rather than chosen. A session starts in the one workspace share's mount point because there is exactly one, so no rule was ever needed; with a set, the absence of a rule becomes a behavior nobody decided. [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) already records a neighbouring rough edge — a subdirectory of the workspace starts at the workspace root — which is the same seam and should be settled with it rather than around it.

This slice depends on [slice 019](../019-declared-mounts-reach-the-guest/README.md) and cannot start before it. Declared mounts do not reach the guest today, so there is no share list to grow and no trial here could distinguish a mirroring defect from the inertness underneath it.
