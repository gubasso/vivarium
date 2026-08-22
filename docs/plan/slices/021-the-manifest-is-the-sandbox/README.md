# 021 — The manifest is the sandbox

## Goal

Key a sandbox on the manifest that describes it rather than on the directory it was started from, so the projects a manifest declares share one VM by construction and the machinery that synthesized a per-directory identity can be deleted.

## Appetite

4 implementation sessions.

## Core

`<sandbox-id>` is the manifest name, every per-sandbox state and runtime path keys on it, resolution from a working directory derives the sandbox from the workspaces manifests declare and refuses a directory a second manifest claims by naming both, and the project-local marker and old identity index are gone.

## In scope

Ordered, because the decision governs the rekey and the rekey governs the sweep.

1. Enact [`ADR-0107`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md), which supersedes [`ADR-0011`](../../../decisions/ADR-0011-config-read-only-binding-in-state.md), [`ADR-0029`](../../../decisions/ADR-0029-project-identity-and-marker.md), [`ADR-0043`](../../../decisions/ADR-0043-identity-marker-lifecycle.md), and [`ADR-0054`](../../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md): amend N7, N9, and N21 to what it decided, and advance its status.
2. Rekey the state root, the runtime root, and volume identity from the old project key to the manifest name, keeping the `<target>` component untouched.
3. Invert the registry. It stops being the written home of a project-to-manifest binding and becomes a derived index from declared workspaces back to the manifest that declares them, rebuildable after deletion and invalidated when a manifest changes.
4. Refuse a second owner. Ownership is single-valued under ADR-0108, so a directory a second manifest declares as a workspace is refused at `78` with both manifests named, which is what keeps resolution from a working directory unique and needs no disambiguating flag. The same directory mounted by any number of manifests stays legal and is exercised rather than refused.
5. Delete the identity machinery: the project-local marker, the identity index, suffix minting, and the move-versus-copy resolution. N9 loses its exception and becomes absolute.
6. Surface a sandbox whose manifest is gone. Renaming or deleting a manifest strands that sandbox's generations and volumes, so the orphan is reported rather than left to be discovered.
7. Sweep the trials. Delete the old collision-suffix trial; replace the `workflow_01` binding legs with manifest/workspace resolution coverage and rename the volume-list precondition trial for the manifest it now requires.
8. Sweep the documents: delete the former project-identity specification, rewrite the resolution chapter of [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md), and move the rows in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md).

## Out of scope

- Sharing a volume between two sandboxes. Rekeying gives one sandbox one set of volumes; a volume spanning sandboxes is a separate question and a block device attached to two live guests is not admissible.
- Any per-invocation path expansion, `${PWD}` included.
- Attaching a workspace to a running sandbox.
- Migrating state a user already has under the old key. A `viv destroy` and a rebuild is the supported path, which is what disposability is for.
- Ordered remainder, cut first when the appetite binds: item 6's orphan surfacing, and the guide sweep item 8 implies.

## Governed by

- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — defines the roots, the registry, and the lock order items 2 and 3 change.
- The former project-identity specification — defined the identity this slice replaces; item 8 deletes it after its surviving target-component rules move to spec/02.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N7, N9, and N21, which item 1 amends.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — defines the teardown boundary item 5 changes when the marker goes.
- [`../../../decisions/ADR-0011-config-read-only-binding-in-state.md`](../../../decisions/ADR-0011-config-read-only-binding-in-state.md) — fixes the binding and precedence item 1 supersedes.
- [`../../../decisions/ADR-0029-project-identity-and-marker.md`](../../../decisions/ADR-0029-project-identity-and-marker.md) — fixes the identity item 1 supersedes.
- [`../../../decisions/ADR-0043-identity-marker-lifecycle.md`](../../../decisions/ADR-0043-identity-marker-lifecycle.md) — fixes the marker lifecycle item 5 removes.
- [`../../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md`](../../../decisions/ADR-0052-state-root-file-layout-and-schema-visibility.md) — fixes which state shapes are supported interfaces, which item 3 changes for the registry.
- [`../../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md`](../../../decisions/ADR-0053-state-file-atomicity-and-lock-ordering.md) — fixes the total lock order, from which item 5 removes a rung.
- [`../../../decisions/ADR-0040-manifest-is-the-personal-layer.md`](../../../decisions/ADR-0040-manifest-is-the-personal-layer.md) — fixes why a literal host path is legal in a manifest, which is what makes item 3 possible.
- [`../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) — fixes the key, the inversion, and the deletions this whole slice enacts.
- [`../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) — fixes the single-owner rule item 4 enforces and the declaration item 3 derives its index from.
- [`../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md`](../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md) — fixes the code and the message shape item 4's refusal reuses.

## Acceptance

When two projects are declared as workspaces in one manifest, `viv start` from either SHALL reach one sandbox, and a second `viv start` from the other SHALL be a no-op against the running VM (N15).

When a sandbox's state and runtime paths are inspected, they SHALL key on the manifest name and SHALL carry the `<target>` component unchanged.

If a directory is declared as a workspace by more than one manifest, then resolution SHALL fail closed at `78`, and the message SHALL name both manifests.

When one directory is declared as a workspace by one manifest and mounted by others, resolution SHALL reach the owning manifest's sandbox, and a trial SHALL assert that the mounting manifests are not candidates.

If the derived index is deleted, then the next command SHALL rebuild it from the manifests alone and SHALL reach the same sandbox.

When a sandbox has been started, no file SHALL be written into the project's own tree, and a trial SHALL assert the tree is unchanged (N9, without exception).

If a manifest that owns retained state is renamed or removed, then that state SHALL be reported as orphaned rather than left unreachable and unnamed.

## Rabbit holes

- Keeping the marker as a fallback because deleting it feels risky — escape: two sources of identity is the condition the current resolution algorithm exists to arbitrate, and carrying one of them forward keeps the algorithm this slice deletes.
- Letting the derived index become state again by storing something it cannot rederive — escape: if a field cannot be recomputed from the manifests, it does not belong in the index, and the acceptance clause that deletes the index is what proves it.
- Writing a migration for existing state — escape: the sandbox is disposable and `viv destroy` followed by `viv start` is the general recovery path; a migration is machinery for a property the product does not claim.
- Reopening the sharing model because manifests now key VMs — escape: manifests stay personal and sharing stays on pieces; that split is exactly what makes literal paths legal in a manifest and the index derivable.
- Recording the owning manifest in a project-local file because deriving it feels indirect — escape: that file reinvents the deleted marker, and item 5's prize is N9 without an exception; the manifests already answer the question.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, `ADR-0107` carries this slice as its enactment and the records it supersedes carry it back, the former project-identity specification is gone with its content absorbed or dropped by decision, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-18, at shaping, before any work started. What follows is the reasoning behind the items above, recorded because it would otherwise be re-derived by whoever picks this up, and because two of its findings are what bound the slice.

The first finding is a constraint on the whole design: sharing manifests and deleting the registry are mutually exclusive. If a manifest is shared between users it enters the shared class, N11 refuses a literal personal path in it, and the host paths must then live in some per-user machine-local file — which is the registry's own job description. The resolution is to keep manifests personal and share pieces instead, which is what the specification already prescribes ([`ADR-0040`](../../../decisions/ADR-0040-manifest-is-the-personal-layer.md), [`../../../reference/spec/07-secrets-and-config-sharing.md`](../../../reference/spec/07-secrets-and-config-sharing.md)) and what is already implemented and covered by `workflow_03_team_shared_and_personal_override`. With manifests personal, literal paths are legal in them, the manifest records its own workspaces, and item 3's inversion becomes sound. A future proposal to distribute manifests reopens this and must reopen item 3 with it.

The second finding is that no portable variable can name the working directory, and item 4 exists because of it. `${HOME}` and the four XDG names are shareable because each takes exactly one value per user; a name that varies per invocation makes the mount set a function of the call rather than of the configuration, which forces identity to include the call and defeats the sharing. So the workspaces are declared, and the cost of declaring them is that two manifests can name one directory — a collision the current path-keyed registry makes structurally impossible, which is why the ambiguity refusal is new work rather than a carried-over check.

The simplification is the reason to do this at all, and it is larger than the feature. Manifest names are already unique by construction, validated kebab-case, resolved against one library, with a per-manifest directory in the config root that already holds the team override lock. So identity needs no derivation: sanitization, the length cap, collision suffixes, the marker, the identity index, the global mint lock, and the whole move-versus-copy-versus-clone resolution table all become unnecessary rather than being ported. The former `src/config/identity.rs` is the largest single body of code this slice removes.

The appetite is four sessions because this is not additive. The resolution layer is rewritten, [`../../../../src/config/registry.rs`](../../../../src/config/registry.rs) inverts, and item 7's trial sweep touches trials that currently pass — so the slice spends a session on work that produces no new capability and is nonetheless not optional.

This slice depends on [slice 020](../020-many-workspaces-in-one-sandbox/README.md), which depends on [slice 019](../019-declared-mounts-reach-the-guest/README.md). The user-visible capability arrives with 020; this slice is what makes it cheap to key and removes the machinery the old key needed.

Reshaped 2026-08-20, before any work started. Item 1 stopped being the decision and became its enactment: [`ADR-0107`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) records the key, so nothing here is blocked and the question that blocked it has left [`../../open-questions.md`](../../open-questions.md) through that exit. The two findings above are unchanged, and are now the reasoning that record was written from.

Item 4 changed shape rather than size. The ambiguity it was going to arbitrate at resolution time is settled a layer earlier: [`ADR-0108`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) splits declaration into a `[[workspaces]]` table that at most one manifest may claim a directory through, and a `[[mounts]]` table any number may. So a directory lies in exactly one sandbox the way it lies in exactly one git repository, overlap stays available where a user wants it, and no flag has to select between candidates because there is never more than one. What replaces the disambiguation is a refusal — a second manifest claiming an owned workspace — which is new work of about the same size, so the appetite holds at four sessions.

Started 2026-08-22 after slice 020 closed. The continued-round host check found `/dev/kvm`, an absolute writable `/run/user/1000`, and about 1.7 TiB free on the configured drive. The user manager was operational but answered `degraded` because six unrelated backup units were failed; no `vivarium-*` unit was loaded. Every heavy lane remained behind [`tests/host/heavy-run`](../../../../tests/host/heavy-run).

Item 2 landed before the registry inversion, preserving the slice's load-bearing order. The manifest name now keys every state, data, cache, runtime, volume, generated-flake, build-record, and transient-unit path while `<target>` remains unchanged. The launch contract calls the field `sandboxId` and schema `11` moves together in Rust, Nix, and the independent contract assertion. Resolution refuses manifest names over 48 bytes, while `Runtime::locate` computes the rendered `control.sock` path and refuses the Linux `sun_path` overflow separately.

The handoff asked phase 4 to advance ADR-0107. This run deliberately defers that status to the final slice checkpoint: the record also decides the derived-index inversion and identity deletion, which do not exist until items 3 through 5 land. The four supersession back-references and four amendment notes already present were verified and not duplicated. N7 moves with the rekey; N9 and N21 wait for identity deletion, and N13 waits for removal of the two authored-binding commands.

Items 3 and 4 replace the authored state-root registry with the lockless cache-root `workspace-index.json`. It stores only manifest names, paths, mtimes, and unexpanded workspace source tokens; deletion, malformed content, or a changed library snapshot rebuilds it through same-directory stage, file flush, rename, and parent-directory flush. A leftover `registry.toml` is ignored byte-for-byte. The published `viv config --json` source spelling changes from `registry` to `derived`, recording that the third precedence rung is recomputed ownership rather than authored state. The two authored-binding commands have no grammar or dispatch. N13 now has no sanctioned user-surface write; N9 and N21 still wait for item 5's identity deletion.

Item 5 deletes the project identity module, marker, index, suffix minting, and their global lock. N9 is now absolute and N21 has no subject. The first-start acceptance snapshots the complete workspace tree and proves it stays byte-identical, including the continued absence of any tool-owned entry. The test-only project-key helper and marker assertions moved forward from item 7 because deleting the production sanitizer export otherwise leaves the phase-6 full-suite checkpoint uncompilable; the identity-only workflow itself remains for item 7's deletion.

Item 6 is present rather than cut. `viv doctor` compares manifest-name directories retained under both state and data with the current manifest library and reports every absent owner and retained path without deleting anything. This preserves ADR-0054's report-rather-than-reap judgement after authored bindings disappear. The catalog replaces the obsolete identity-index parse probe with `state-manifest-orphans`, keeping the catalog size and preflight subset unchanged.

Items 7 and 8 are present rather than cut. The workflow table now has 26 entries: the collision-suffix subject is gone, the selection and volume-precondition trials name the manifest/workspace model, and the exact virtualization group moved with the harness table. The project-identity specification is deleted after its surviving `<target>` rule moved to spec/02; current guides, subsystem explanation, command/spec owners, comparison notes, examples, slides, and historical slice links were swept rather than preserving dead commands or markers. `docs/reference/implementation-status.md` now has 18 `Implemented` rows by recounting the table. ADR-0107 and ADR-0109 advance to `Implemented`; ADR-0108 remains `Implemented`.

The final profile ran twice with `VIVARIUM_TEST_REQUIRE=1` and a realized `VIVARIUM_AGENT_RUNNER`, making every capable-host gate a test rather than a skip: 55 of 55 passed in 565.67 seconds, then 55 of 55 in 553.08 seconds. Supplying those two variables is an explicit addition to the literal `just test-pre-push` command in the handoff: without the runner, requiring the standalone guest-agent binary fails for a harness reason the owning lane normally supplies. The schema-11 `first-microvm` contract built. The first hook pass applied its Rust and Markdown formatters and exposed one missing fence language; after that local repair, the complete pre-commit hook set passed. No slice item or ordered remainder was cut, and phases 4 through 7 ran as one merged pass under the four-session appetite.

Reviewed 2026-08-22 after the merged pass, and eight findings were repaired before the checkpoint closed. Seven were published-surface rather than behaviour: three new diagnostic ids had no home or the wrong one — `manifest.name-too-long` was catalogued as `state.name-too-long`, which is an id no `grep` would ever match, while `manifest.workspace-undeclared-directory` and `host.runtime-socket-path-too-long` were unfiled — and the `working-directory-declared` probe, which is half of ADR-0109's two-consumer enactment, was missing from the doctor catalog. Three living comparison documents still asserted the marker-anchored identity this slice deletes, one of them as a `built` capability. The eighth is the one worth naming as a rule rather than a repair: the link sweep had repointed eight link targets inside five frozen decision bodies from spec/15 to spec/02 while leaving the visible text naming spec/15, so each sentence cited one document and delivered the reader to another. A frozen body's link target stays as recorded; the permitted repair is to unlink the text, which is what was done, because a dead reference a reader recognises as history costs less than a live one that silently substitutes a different source.

Gates after those repairs, all through [`tests/host/heavy-run`](../../../../tests/host/heavy-run): the `pre-push` profile green twice at 571 and 549 seconds, `nix flake check` over all four contract lanes, and the complete pre-commit hook set. Every registered probe id now appears in the spec catalog, and no living document outside the frozen records still names the marker, `viv init`, or `viv unbind`.
