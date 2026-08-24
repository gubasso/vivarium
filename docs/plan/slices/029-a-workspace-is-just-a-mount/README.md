# 029 — A workspace is just a mount

## Goal

Delete the launch channel that carried a workspace's host path into the guest, so a declared tree reaches the sandbox through the one sharing mechanism every other declaration already uses, and the guest stops knowing what a workspace is.

## Appetite

4 implementation sessions.

## Core

A manifest's `[[workspaces]]` rows compile to ordinary declared mounts whose `target` equals their expanded `source`, every refusal a user meets keeps its exit code and its diagnostic id, and `nix/workspace-mirror.sh`, the percent-encoder, the `ws` tag family, and the `vivarium.workspace.<tag>=` kernel parameters are gone rather than unused.

## In scope

Ordered, because each item is what makes the next one safe to delete.

1. Move the workspace rules into Rust, ahead of everything they guard. The guest-owned path list, the equal-or-nested pair check, and the source-defect ids leave [`../../../../src/launch/spec.rs`](../../../../src/launch/spec.rs) and [`../../../../src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs) for one module the resolution routine calls, so a defect is decided from manifest text and the environment before any evaluation. Every exit code and every published id is unchanged; only where they fire moves.
2. Emit a workspace as a mount. [`../../../../src/config/flake.rs`](../../../../src/config/flake.rs) already renders the manifest into the generated flake, so each `[[workspaces]]` row is expanded once at resolution and written as a `vivarium.mounts` row with `target` equal to `source` and `readonly = false`. One resolved list, computed once, reaches both the derived index and the flake, because two callers deriving it separately is how they disagree.
3. Delete the workspace from Nix. The `vivarium.workspaces` option, the `ws` tag family, `workspacesInternalRoot`, the `vivarium-workspace` unit, the `workspace` share origin, and `--workspace` in [`../../../../nix/runner.sh`](../../../../nix/runner.sh) go; `nix/mount-bind.sh` absorbs the case and owns the rationale it currently defers to the deleted file.
4. Delete the launch channel. `encode_cmdline_path` and the workspace half of `compose_cmdline` in [`../../../../src/launch/supervisor.rs`](../../../../src/launch/supervisor.rs) go with the parameters they wrote. The boot record keeps a tag-free list of workspace paths, because `reusable()` compares no store path and that list is the only thing detecting a changed workspace set on a path that deliberately evaluates nothing.
5. Keep the reader's view. `viv config eval` and `viv config sources` still show a `workspaces` block naming the declared, unexpanded sources, injected by the CLI, since no Nix option stands behind it any more.
6. Move the invariants with the code. N5 retires, N16 loses its launch-channel sentence, N19 gains a workspace carve-out shaped like its volume one, and N3 and N10 each gain the sentence that names the host path now in the build output.
7. Sweep the verification surface: the `tests/nix` contract, the measurement legs that create handshake directories inside the workspace share, and every host lane that passes `--workspace`.
8. Sweep the prose that describes the deleted mechanism, and take the exit on [`../../open-questions.md`](../../open-questions.md) `Q-019`, whose JSON boundary this slice removes.

## Out of scope

- Ownership resolution. Which manifest owns a directory is decided exactly as [slice 021](../021-the-manifest-is-the-sandbox/README.md) left it, from manifest text, in Rust, refused at `78`. Nothing here touches the derived index.
- The authored surface. A user still writes `[[workspaces]]` with a `source` and no `target`; only what it compiles to changes.
- Any backwards compatibility. No shim reads the old kernel parameter, no deprecated option remains, and no comment explains what used to be there.
- Collapsing `[[mounts]]` and `[[workspaces]]` into one table. The two verbs claim different things and `ADR-0108` stands.
- Ordered remainder, cut first when the appetite binds: item 8's prose sweep beyond the specification pages, and the build assertion named in the second rabbit hole.

## Governed by

- [`../../../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md`](../../../decisions/ADR-0110-the-workspace-is-an-ordinary-mount.md) — the decision this slice enacts, and the record naming the purity cost it accepts.
- [`../../../reference/spec/08-invariants-and-guarantees.md`](../../../reference/spec/08-invariants-and-guarantees.md) — carries N3, N5, N10, N16, and N19, which item 6 moves.
- [`../../../reference/spec/04-composition-and-determinism.md`](../../../reference/spec/04-composition-and-determinism.md) — defines the build and launch channels a workspace changes sides of.
- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — defines the workspace mount and the mount schema it compiles into.
- [`../../../reference/spec/12-exec-and-shell.md`](../../../reference/spec/12-exec-and-shell.md) — fixes the session directory and the ensure-running comparison item 4 changes.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — draws the `65`/`78` boundary item 1's refusals must stay on.
- [`../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md`](../../../decisions/ADR-0108-a-workspace-is-owned-by-one-manifest.md) — fixes the authored table and the ownership rule this slice preserves.
- [`../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md`](../../../decisions/ADR-0109-an-undeclared-working-directory-is-refused.md) — fixes the refusal and message contract item 1 joins rather than changes.
- [`../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes the mount schema item 2 compiles into.

## Acceptance

When a manifest declares several project trees, `viv start` SHALL boot one VM in which each tree is readable and writable at its own host path, and a trial SHALL round-trip an edit through each. This is slice 020's promise and it SHALL hold unchanged.

The built guest SHALL declare no share tagged outside the `store` and `mnt` families, and the launch contract SHALL name no `workspace` origin. A contract check SHALL assert this against the guest's own fstab and unit set rather than against a restated literal.

The guest kernel command line SHALL carry no `vivarium.workspace` parameter, and no guest unit SHALL bind a workspace.

If a declared workspace equals, contains, or lies under a path the guest owns, or if two declared workspaces nest in each other, then the command SHALL refuse at `78` with the same diagnostic id it carries today, before any evaluation. A trial SHALL assert the refusal fires on `viv config eval` as well as on `viv start`.

When a manifest declares a workspace, `viv config eval` SHALL show it under `workspaces` as the user wrote it, and under `mounts` as it was expanded.

When a running VM's manifest gains or loses a workspace, the next invocation SHALL detect the changed set without evaluating.

## Rabbit holes

- Rebuilding the ownership model because the compiled form moved — escape: item 2 changes what a `[[workspaces]]` row becomes and nothing about what it claims; the derived index and the `78` refusal are untouched.
- Letting a workspace-only guard die quietly because `mount-bind.sh` has no equivalent — escape: each of the four guards in the deleted unit is either re-sited in Rust, argued as covered by an existing build assertion, or recorded as a deliberate reduction beside the boundary that no longer carries it. None is dropped in silence.
- Re-deriving the `mnt<i>` index anywhere outside the guest module — escape: the measurement legs look their mount point up off the guest's own share list, because a second derivation of one index is the exact failure the contract exists to catch.
- Treating the purity loss as a footnote — escape: one manifest now evaluates to different store outputs on two machines; `ADR-0110` says so, and N3 and N10 carry the sentence rather than leaving the reader to infer it.
- Closing the rendering-lane question in `tests/host/exec-and-shell-check` while rewriting its comment — escape: this slice changes the noun in that comment and nothing else; the question stays open.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial or contract check it names, `nix/workspace-mirror.sh` is deleted and no file references it, `ADR-0110` carries this slice as its enactment while `ADR-0009` and `ADR-0100` carry it as their successor, `Q-019` has left [`../../open-questions.md`](../../open-questions.md) through its recorded exit, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) carries the rows this slice moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-24, at shaping, before any work started.

The premise this slice removes was true when it was written and stopped being true two slices ago. `ADR-0009` kept the working directory's host path out of the build because that directory was the invoking one — unknowable until the tool ran. `ADR-0108` made a workspace a declaration, and a declaration is knowable at build time. Everything the launch channel exists to carry has therefore been sitting in the manifest since slice 020, which is also already compiled into the generated flake and already in the store under N10. What the channel still buys is one build usable under two different `$HOME` values for one manifest, and the price is a percent-encoder, a decoder, a tag family, a second guest unit, and a kernel-command-line budget.

The capable-host premises were confirmed before the slice was written rather than assumed: `/dev/kvm` present, `$XDG_RUNTIME_DIR` the absolute writable `/run/user/1000`, and `VIVARIUM_HEAVY_DRIVE` naming an existing directory under an attached drive with about 1.8 TiB free. The systemd user manager answered `degraded` rather than `running`, from twelve unrelated failed `backup-*` units; its bus and transient-unit operations were usable and no `vivarium-*` unit was loaded. This matches what slice 021 recorded on 2026-08-22 and is not promoted into a product premise.

One decision inside the slice is worth naming here because it is not obvious from the items. `target == source` becomes the definition of a workspace row rather than a marker on it. That is what lets the supervisor rebuild the workspace list from the launch specification alone, with no new argument channel and no field whose only job is to say what a row used to be called. It also means a canonicalization difference between generation and launch would silently unmake a workspace, so the boot record is checked against what resolution computed rather than trusted, and a disagreement is one diagnostic instead of a cold start on every invocation.

Implemented 2026-08-24 in one pass. What follows is what the work found that the shaping did not name.

Items 1 and 2 landed as one change rather than two. The plan had the rules move into Rust additively, with both copies live, so the tree would be green between them. That would have meant duplicating the guest-owned list and the pair check for one checkpoint, which is the opposite of what this slice is for, so the move and the collapse landed together and the checkpoint is the whole Rust side compiling and passing.

Three things moved that the shaping did not anticipate. `target == source` became the definition of a workspace rather than a marker on one, which is what let the supervisor rebuild the boot record's workspace list from the launch specification with no new argument channel; the launch contract publishes each declared share's `target` for that reason, read from the guest module's own bind table through a new internal option rather than re-derived from the option list, because a second derivation of the `mnt<index>` order is exactly the silent reclassification the contract exists to catch. `boot.json` swapped its tag-to-path map for a sorted path list, since nothing mints a workspace tag any more, and `reusable()` compares the two sorted sets — it remains the only thing that can detect a changed declaration on a path that deliberately evaluates nothing, because that routine compares no store path. And the manifest's authored `[[mounts]]` rows are rendered before the workspace rows, so adding a workspace does not move a mount's `mnt<index>`; the first rendering put workspaces first and moved `mnt0` out from under `workflow_22_file_mount_serves_only_its_file`.

Two trials changed what they assert, and both changes are the slice working rather than the slice being accommodated. `workflow_17_declared_mounts_refusals` had `viv status` answer `built` after `viv start` refused an unmirrorable workspace; the refusal is decided at resolution now, so every resolving verb answers the same way and the trial asserts that instead. `workflow_17_linked_worktree_reaches_main` declared its main repository twice, once as a workspace and once as a `[[mounts]]` row mirroring itself; those were two mechanisms and are now one, so the pair is a duplicate target refused at `65`. The redundancy was always there and was invisible while two units bound the same path and the later one silently won.

One finding is a regression rather than a repair, and it is recorded rather than worked around. `workflow_03_team_shared_and_personal_override` proved that a piece declaring `vivarium.workspaces` was refused at `65` naming the layer. With the option deleted there is nothing left to read: [`../../../../nix/vivarium-report.nix`](../../../../nix/vivarium-report.nix) evaluates each layer with `_module.check = false` so an ordinary NixOS layer is not fatal in the provenance view, and an undeclared `vivarium.*` is ignored along with it. A piece written against the old spelling is therefore inert instead of refused. The trial now pins that behaviour, `ADR-0110` carries it as a `Bad` consequence, and it left as [`Q-034`](../../open-questions.md).

[`Q-019`](../../open-questions.md) left through its exit and is removed. Its subject was a workspace path crossing into the launch specification through `jq --arg`, where a non-UTF-8 byte became U+FFFD before the byte-exact encoder saw it, so the guest mirrored the tree at a path differing from the host's and nothing reported it. There is no such crossing now. The path passes through `nix_string` in [`../../../../src/config/flake.rs`](../../../../src/config/flake.rs) into the generated flake instead, where an unrepresentable byte is a defect the evaluation reports rather than a silently different guest path.

The purity lane narrowed with the change and the narrowing is recorded in [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md). Its metamorphic half built one manifest from two host paths and demanded one derivation, which now asserts the opposite of what the product promises. What it still proves is that nothing resolving at launch reaches a build input, which is the whole of N19 as it now reads. N5 is retired rather than weakened: its subject, launch-time injection of the working-directory path, no longer exists.

The capable-host premises were confirmed before the first command and matched what the shaping recorded: `/dev/kvm` present, `$XDG_RUNTIME_DIR` the absolute writable `/run/user/1000`, about 1.8 TiB free on the configured drive, and a user manager answering `degraded` from twelve unrelated failed `backup-*` units with no `vivarium-*` unit loaded. Every build, evaluation, and trial ran through [`../../../../tests/host/heavy-run`](../../../../tests/host/heavy-run).

Reviewed 2026-08-24, and the pass paid for itself. Three findings, two of them defects the whole green suite had walked past.

The first is the one worth keeping as a lesson. `workspace::resolve` returned its paths in declaration order and the supervisor sorted its copy, while `reusable()` compared the two directly — so a manifest declaring `/z` before `/a` produced a boot record holding one order and a resolution holding the other, and every invocation after the first refused a healthy VM as foreign. Nothing caught it because every fixture in the suite happens to declare its trees in ascending order, where a sorted set and a declaration-ordered list are the same list. Both sides now go through one `canonicalize_set`, and the regression test declares in descending order for exactly that reason.

The second is the ambiguity the predicate rests on, found rather than reasoned about. `target == source` marks a mirrored tree, and a `[[mounts]]` row may legitimately name a path at itself — so such a row landed in the boot record's workspace set while resolution, which reads `[[workspaces]]` alone, never put it there. Same symptom as the first and a worse cause. A row that mounts a path at itself is a workspace, so it is refused as one written in the wrong place: against the authored TOML for the manifest's own rows, where the two tables are still distinguishable, and against the merged view for a shared layer's, naming the layer. The manifest's compiled contribution is skipped there because that is where the workspace rows themselves live.

The third is a documentation defect rather than a behavioural one, and it is the rabbit-hole escape above failing to execute. The deleted mirror unit refused a leaf that already existed; `mount-bind.sh` binds over one. The slice claimed each dropped guard was recorded in `ADR-0110` and none was. The reduction is now recorded beside the boundary that stopped carrying it, which is where `AGENTS.md` puts a load-bearing comment, and the escape says so.

A second review round found two more, both regressions in the first round's own repairs, and together they moved the design to where it should have started.

The first round had closed the identity-mount ambiguity by refusing a row whose target equals its source — textually, against the authored TOML and against a shared layer's contribution. That is an approximation, and the round-2 finding named the gap exactly: `source = "/srv/tree/."` with `target = "/srv/tree"` passes both textual checks and canonicalizes to an identity at launch, where the predicate reads the resolved path. Any spelling that differs on paper and agrees after expansion walks through. The second finding was the same repair's other end: the new refusal returned an ordinary resolution failure, which `viv doctor` discards through its `.ok()`, so the one verb whose job is to report and continue reported nothing.

Both are gone because the approximation is gone. `viv` knows which rows came from `[[workspaces]]` — it resolved them — so it now says so instead of leaving a reader to infer it. `MountPlanKind` gained `Tree`, relayed on the runner's existing `--mount TAG KIND ABS ENTRY` group exactly as `dir` and `file` already are, and `workspace_shares` reads that kind rather than comparing a target to a source. The equality survives as a contract on the rows that claim the kind — `validate` refuses a tree plan that binds anywhere but its own source — which is the honest direction: assert the property on rows that declare it, rather than use the property to guess which rows declare it.

That deletes both textual refusals, their tests, and the whole class of question about which spellings alias. It also leaves the guest untouched: `Tree` rides the kernel command line as `dir`, because a mirrored tree and an ordinary directory mount bind identically, so nothing about a workspace reaches the guest — which is what `ADR-0110` decided and what the first round's repair had started to erode.

The lesson worth keeping is the one the two rounds share. Both defects were the same mistake in two costumes: deriving a fact on one side of a process boundary and re-deriving it on the other, instead of carrying it. The first round found it in an ordering, the second in a classification, and the fix in both cases was to make one side the author and the other a reader.
