# 021 — The manifest is the sandbox

## Goal

Key a sandbox on the manifest that describes it rather than on the directory it was started from, so the projects a manifest declares share one VM by construction and the machinery that synthesized a per-directory identity can be deleted.

## Appetite

4 implementation sessions.

## Core

`<sandbox-id>` is the manifest name, every per-sandbox state and runtime path keys on it, resolution from a working directory derives the sandbox from the workspaces manifests declare and refuses an ambiguous directory by naming its candidates, and the `.vivarium/` marker and the identity index are gone.

## In scope

Ordered, because the decision governs the rekey and the rekey governs the sweep.

1. Record the decision set. It supersedes [`ADR-0011`](../../../decisions/ADR-0011-config-read-only-binding-in-state.md), [`ADR-0029`](../../../decisions/ADR-0029-project-identity-and-marker.md), [`ADR-0043`](../../../decisions/ADR-0043-identity-marker-lifecycle.md), and [`ADR-0054`](../../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md), and amends N7, N9, and N21. Blocked on [`Q-029`](../../open-questions.md), whose exit is this item.
2. Rekey the state root, the runtime root, and volume identity from `<project-id>` to the manifest name, keeping the `<target>` component untouched.
3. Invert the registry. It stops being the written home of a project-to-manifest binding and becomes a derived index from declared workspaces back to the manifest that declares them, rebuildable after deletion and invalidated when a manifest changes.
4. Refuse ambiguity. Two manifests declaring one directory is newly possible — the current registry makes it structurally impossible by keying on the canonical path — so resolution names both candidates and fails closed, with the flag that disambiguates named in the same message.
5. Delete the identity machinery: the `.vivarium/` marker, the identity index, suffix minting, and the move-versus-copy resolution. N9 loses its exception and becomes absolute.
6. Surface a sandbox whose manifest is gone. Renaming or deleting a manifest strands that sandbox's generations and volumes, so the orphan is reported rather than left to be discovered.
7. Sweep the trials. `workflow_02_identity_collision_suffix` has no subject after item 5; the `workflow_01` binding legs and `workflow_07_volume_list_requires_binding` assert a binding that no longer exists in that form.
8. Sweep the documents: delete [`../../../reference/spec/15-project-identity.md`](../../../reference/spec/15-project-identity.md), rewrite the registry chapter of [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md), and move the rows in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md).

## Out of scope

- Sharing a volume between two sandboxes. Rekeying gives one sandbox one set of volumes; a volume spanning sandboxes is a separate question and a block device attached to two live guests is not admissible.
- Any per-invocation path expansion, `${PWD}` included.
- Attaching a workspace to a running sandbox.
- Migrating state a user already has under the old key. A `viv destroy` and a rebuild is the supported path, which is what disposability is for.
- Ordered remainder, cut first when the appetite binds: item 6's orphan surfacing, and the guide sweep item 8 implies.

## Governed by

- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — defines the roots, the registry, and the lock order items 2 and 3 change.
- [`../../../reference/spec/15-project-identity.md`](../../../reference/spec/15-project-identity.md) — defines the identity this slice replaces, and is the page item 8 deletes.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N7, N9, and N21, which item 1 amends.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — defines the teardown boundary item 5 changes when the marker goes.
- [`../../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../../decisions/ADR-0011-config-read-only-binding-in-state.md) — fixes the binding and precedence item 1 supersedes.
- [`../../../decisions/ADR-0029-project-identity-and-marker.md`](../../../decisions/ADR-0029-project-identity-and-marker.md) — fixes the identity item 1 supersedes.
- [`../../../decisions/ADR-0043-identity-marker-lifecycle.md`](../../../decisions/ADR-0043-identity-marker-lifecycle.md) — fixes the marker lifecycle item 5 removes.
- [`../../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md`](../../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md) — fixes which state shapes are supported interfaces, which item 3 changes for the registry.
- [`../../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md`](../../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md) — fixes the total lock order, from which item 5 removes a rung.
- [`../../../decisions/ADR-0040-manifest-is-the-personal-layer.md`](../../../decisions/ADR-0040-manifest-is-the-personal-layer.md) — fixes why a literal host path is legal in a manifest, which is what makes item 3 possible.

## Acceptance

When two projects are declared as workspaces in one manifest, `viv start` from either SHALL reach one sandbox, and a second `viv start` from the other SHALL be a no-op against the running VM (N15).

When a sandbox's state and runtime paths are inspected, they SHALL key on the manifest name and SHALL carry the `<target>` component unchanged.

If a working directory is declared by more than one manifest, then resolution SHALL fail closed, and the message SHALL name every candidate and the flag that selects one.

If the derived index is deleted, then the next command SHALL rebuild it from the manifests alone and SHALL reach the same sandbox.

When a sandbox has been started, no file SHALL be written into the project's own tree, and a trial SHALL assert the tree is unchanged (N9, without exception).

If a manifest that owns retained state is renamed or removed, then that state SHALL be reported as orphaned rather than left unreachable and unnamed.

## Rabbit holes

- Keeping the marker as a fallback because deleting it feels risky — escape: two sources of identity is the condition the current resolution algorithm exists to arbitrate, and carrying one of them forward keeps the algorithm this slice deletes.
- Letting the derived index become state again by storing something it cannot rederive — escape: if a field cannot be recomputed from the manifests, it does not belong in the index, and the acceptance clause that deletes the index is what proves it.
- Writing a migration for existing state — escape: the sandbox is disposable and `viv destroy` followed by `viv start` is the general recovery path; a migration is machinery for a property the product does not claim.
- Reopening the sharing model because manifests now key VMs — escape: manifests stay personal and sharing stays on pieces; that split is exactly what makes literal paths legal in a manifest and the index derivable.
- Rewriting the trial suite around the new key before the key is settled — escape: item 1 is blocked on Q-029 and everything after it depends on the answer.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, item 1's decisions are recorded with this slice linked as their enactment, [`Q-029`](../../open-questions.md) carries its exit, [`../../../reference/spec/15-project-identity.md`](../../../reference/spec/15-project-identity.md) is gone with its content absorbed or dropped by decision, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-18, at shaping, before any work started. What follows is the reasoning behind the items above, recorded because it would otherwise be re-derived by whoever picks this up, and because two of its findings are what bound the slice.

The first finding is a constraint on the whole design: sharing manifests and deleting the registry are mutually exclusive. If a manifest is shared between users it enters the shared class, N11 refuses a literal personal path in it, and the host paths must then live in some per-user machine-local file — which is the registry's own job description. The resolution is to keep manifests personal and share pieces instead, which is what the specification already prescribes ([`ADR-0040`](../../../decisions/ADR-0040-manifest-is-the-personal-layer.md), [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md)) and what is already implemented and covered by `workflow_03_team_shared_and_personal_override`. With manifests personal, literal paths are legal in them, the manifest records its own workspaces, and item 3's inversion becomes sound. A future proposal to distribute manifests reopens this and must reopen item 3 with it.

The second finding is that no portable variable can name the working directory, and item 4 exists because of it. `${HOME}` and the four XDG names are shareable because each takes exactly one value per user; a name that varies per invocation makes the mount set a function of the call rather than of the configuration, which forces identity to include the call and defeats the sharing. So the workspaces are declared, and the cost of declaring them is that two manifests can name one directory — a collision the current path-keyed registry makes structurally impossible, which is why the ambiguity refusal is new work rather than a carried-over check.

The simplification is the reason to do this at all, and it is larger than the feature. Manifest names are already unique by construction, validated kebab-case, resolved against one library, with a per-manifest directory in the config root that already holds the team override lock. So identity needs no derivation: sanitization, the length cap, collision suffixes, the marker, the identity index, the global mint lock, and the whole move-versus-copy-versus-clone resolution table all become unnecessary rather than being ported. [`../../../../src/config/identity.rs`](../../../../src/config/identity.rs) is the largest single body of code this slice removes.

The appetite is four sessions because this is not additive. The resolution layer is rewritten, [`../../../../src/config/registry.rs`](../../../../src/config/registry.rs) inverts, and item 7's trial sweep touches trials that currently pass — so the slice spends a session on work that produces no new capability and is nonetheless not optional.

This slice depends on [slice 020](../020-many-workspaces-in-one-sandbox/README.md), which depends on [slice 019](../019-declared-mounts-reach-the-guest/README.md). The user-visible capability arrives with 020; this slice is what makes it cheap to key and removes the machinery the old key needed.
