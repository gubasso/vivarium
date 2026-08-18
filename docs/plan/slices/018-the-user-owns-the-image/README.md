# 018 — The user owns the image

## Goal

Put every component of the built VM under the user's control: vivarium ships a base to start from, and the user decides what that base resolves to and when it moves, with no vivarium release in the path.

## Appetite

4 implementation sessions.

## Core

A user can state what the guest's build inputs resolve to, move them with `viv update` on their own schedule, and receive a component fix without waiting for a vivarium release — while exactly one effective lockfile still pins the build.

## In scope

Ordered; when the appetite binds, cut from the bottom.

1. Decide the ownership line and record it, because two accepted rules currently pull against each other. [`ADR-0102`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) established that vivarium owns the tool and not the build; [`spec/03`](../../../reference/spec/03-artifact-model.md) reserves the `nixpkgs` and `microvm` input names and refuses an artifact that declares one at `78`. The reservation is right about collision and wrong about ownership: it leaves the user unable to say which channel their own guest is built from. The decision names what a user may redirect, what stays vivarium's, and why a redirect is not the reserved-name collision the refusal was written for.
2. Give the user the knob the decision sanctions: a stated place to say what a baseline input resolves to, without a second pin file. A redirect names a flake reference and never a revision, so pinning stays the lockfile's alone ([`ADR-0074`](../../../decisions/ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md)) and "exactly one effective lockfile" stays literally one ([`ADR-0059`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)).
3. Implement `viv update`. It is `Designed` in [`implementation-status.md`](../../../reference/implementation-status.md), no slice owns it, and without it nothing else in this slice is reachable by a user: naming inputs, reporting each one's before and after, building nothing, and refusing at `78` under a team override lock ([`ADR-0062`](../../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md)).
4. Make the shipped base a starting point rather than a fixture. A user can take it into their own config root, stop tracking vivarium's copy, and keep building — and the guide says how, in those terms. Nothing is published anywhere; this is a copy the user then owns.
5. Re-examine the evaluation-time feature floors [`ADR-0049`](../../../decisions/ADR-0049-backend-is-a-closure-member.md) turned into assertions — free page reporting on the balloon, Landlock in the hardened profile, block-device discard. A user moving a pin must meet a refusal that names the floor, the component, and the pin, rather than a build failure from inside a Nix evaluation. A floor stays a floor; what changes is what the user is told.
6. Amend the owning documents in the same change: [`spec/01`](../../../reference/spec/01-command-surface.md) for the update verb as implemented, [`spec/02`](../../../reference/spec/02-config-and-xdg-layout.md) for where a redirect is stated and what the lock holds, [`spec/03`](../../../reference/spec/03-artifact-model.md) for the reservation as it ends up, [`spec/04`](../../../reference/spec/04-composition-and-determinism.md) for the generated flake's inputs, and [`configuration-and-composition.md`](../../../explanation/configuration-and-composition.md) for the resulting topology.
7. Add the trial that proves the claim end to end: as the user, redirect a baseline input, run `viv update`, rebuild, and show the guest carrying the moved component with no vivarium release involved and no other pin moved. The rebuild realises a fresh guest closure, so the lane calls [`../../../../tests/host/disk-preflight`](../../../../tests/host/disk-preflight) first and names the space it needs, per the capacity convention in [`../../../../AGENTS.md`](../../../../AGENTS.md).

## Out of scope

- Automatic pin movement. [`ADR-0078`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) rejected it as the unannounced input jump N3 forbids, and user control is the point of this slice rather than an argument against that.
- Changing advisory response windows, or the security role and its staffing posture ([`ADR-0079`](../../../decisions/ADR-0079-security-role-is-held-solo-and-windows-are-targets.md)).
- Moving a team override lock from inside vivarium; it stays the team's own act.
- The inner project environment's own store or inputs, which [`ADR-0084`](../../../decisions/ADR-0084-the-inner-layer-provisions-its-own-store.md) already rules the project provisions itself.
- Publishing or distributing images, which [`spec/00`](../../../reference/spec/00-goals-and-non-goals.md) states as a non-goal.
- Any change to how a manifest binds, resolves, or merges.

## Governed by

- [`../../../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) — fixes that vivarium supplies the tool and is never its own build input; this slice extends the same line to the image.
- [`../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — fixes that only `viv update` moves a pin and no build re-resolves.
- [`../../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md`](../../../decisions/ADR-0062-override-lock-is-per-manifest-and-update-refuses.md) — fixes the refusal under a team override lock.
- [`../../../decisions/ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md`](../../../decisions/ADR-0073-shared-artifacts-declare-their-own-flake-inputs.md) — fixes how an artifact declares an input, and reserves the baseline names this slice revisits.
- [`../../../decisions/ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md`](../../../decisions/ADR-0074-declared-inputs-are-pinned-by-the-effective-lock.md) — fixes that a declaration carries no revision and exactly one lock pins it.
- [`../../../decisions/ADR-0049-backend-is-a-closure-member.md`](../../../decisions/ADR-0049-backend-is-a-closure-member.md) — fixes that the backend is pinned by the lock rather than probed, which is why user control over the lock is control over the backend.
- [`../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) — owns the response model whose stated cost, that remediation waits on a release plus a user's update, this slice reduces to the update alone.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — owns `viv update` and its reporting contract.
- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — owns lock ownership, override precedence, and the config root.
- [`../../../reference/spec/03-artifact-model.md`](../../../reference/spec/03-artifact-model.md) — owns the artifact surface and the reserved input names.
- [`../../../reference/spec/04-composition-and-determinism.md`](../../../reference/spec/04-composition-and-determinism.md) — owns the generated flake's inputs and the determinism guarantee.

## Acceptance

When a project states what a baseline input resolves to, the generated flake SHALL use that reference, and the build SHALL remain pinned by exactly one effective lockfile.

When `viv update` runs under the tool-owned lock, it SHALL move only the inputs named, SHALL report each one's before and after, and SHALL NOT build.

When `viv update` runs while a team override lock is in force, it SHALL refuse at `78` naming both files and SHALL write nothing.

When a user moves a guest component's pin and rebuilds, the next `start` SHALL boot a VM carrying the moved component, with no vivarium release required and no unnamed pin moved.

If a moved pin does not satisfy a backend feature floor, the refusal SHALL name the floor, the component, and the pin in force.

When this slice changes a command's state, [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) SHALL be updated in the same change.

## Rabbit holes

- Redirecting a baseline input becomes a second pin file — escape: ADR-0074 owns "exactly one effective lockfile"; a redirect names a reference, never a revision.
- Unreserving the names reopens the collision the reservation prevented — escape: the reservation exists so two artifacts cannot fight over one name; a user redirecting their own project is a different act and the decision in item 1 says so explicitly.
- `viv update` grows into a dependency resolver — escape: it asks Nix to re-resolve declared inputs and reports what moved; resolution stays Nix's.
- The feature floors get relaxed so any pin evaluates — escape: ADR-0049 owns them as assertions and they stay assertions; item 5 changes the diagnostic, not the guarantee.
- The shipped base turns into a second namespace or a published artifact — escape: [`ADR-0061`](../../../decisions/ADR-0061-examples-ship-not-a-second-namespace.md) already ruled that examples ship rather than forming one, and publishing is a stated non-goal.
- The slice drifts into rewriting how manifests compose because the inputs surface is nearby — escape: composition is out of scope above; only the inputs the generated flake carries are in play.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, the item 1 decision reaches `Accepted` with this slice linked as its enactment, `viv update` leaves `Designed` in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) with its trial named, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-17, at shaping, before any work started. What follows is the observation record behind the items above. Each one narrows the slice, and each would otherwise be re-derived by whoever picks this up.

The gap is narrower than "the user has no control", and item 2 must not over-build on the assumption that it is wider. Forward movement along a reference that is already named is designed today: a released vivarium names branches so a user gets today's upstream, pinned from then on by the lock the first evaluation writes, and thereafter only `viv update` moves that pin ([`ADR-0059`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)). Slice 015 settled the other half, recording that the embedded product tree evaluates against the project's own `nixpkgs` and `microvm` so the existing `follows` semantics hold — so the guest's build inputs already belong to the project rather than to the installation ([`../015-the-binary-supplies-itself/README.md`](../015-the-binary-supplies-itself/README.md)). What a user cannot do is say which reference those inputs resolve to, because [`spec/03`](../../../reference/spec/03-artifact-model.md) reserves both names and refuses an artifact declaring one at `78`. Item 2 is therefore a redirect on an input the project already owns, not a re-plumbing of where guest inputs come from, and item 1 exists because that refusal is correct about collision and silent about ownership.

`viv update` is `Designed` in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md) and no slice owned it before this one. It was searched for across every shaped and closed slice; slices 011, 012, 014, and 015 each name the verb, and 015 explicitly leaves it `Designed` and out of its own scope. That absence is why item 3 sits here rather than being assumed available: without the verb, every other item in this slice is unreachable by a user, and it is the largest single reason the appetite is four sessions rather than two.

A consequence worth carrying forward, because it bears on a claim this project makes in public. The backend is a closure member resolved from that same `nixpkgs` ([`ADR-0049`](../../../decisions/ADR-0049-backend-is-a-closure-member.md)), so user control over the `nixpkgs` pin already is user control over the hypervisor's version. Once `viv update` runs, "a backend fix arrives only when vivarium publishes a release" holds only for a fix that needs vivarium's own default reference moved, and fails for one already reachable from the user's own pin. [`ADR-0078`](../../../decisions/ADR-0078-backend-advisory-response-is-a-released-pin-move.md) states the unqualified form as its own `Bad` consequence — remediation is opt-in, so a fix reaches nobody who does not update — and this slice is what makes the qualified form sayable. Two things then go stale when this slice closes and MUST be revisited with it: that ADR's consequence, and the "Security updates come from us" item on the costs slide in [`../../../../slides/slides.md`](../../../../slides/slides.md), which currently tells an audience the unqualified version.

A pin move followed by a rebuild is the same shape [`Q-023`](../../open-questions.md) prices at a cold first boot, so item 7's cost is known before the lane runs rather than discovered inside it. That is the second reason item 7 names the disk preflight rather than leaving capacity to chance.

This slice was shaped on the `docs-site` branch rather than on the branch carrying slices 001 through 015. `docs/plan/` was byte-identical across the two immediately before this change, so the divergence is exactly this directory and the [`milestones.md`](../../milestones.md) row that names it, and carrying both across is all a sync needs. Nothing else in the plan zone moved.

Recorded 2026-08-18, at the merge, before any work started. The slice moved from id `016` to `018` and its item 1 decision's neighbour moved from `ADR-0103` to `ADR-0104`, because the branch this was shaped on and the branch carrying slices 001 through 015 both allocated the next free numbers while diverged: `016 the-human-face` and `017 doctor-catalog` closed on the other side, and `ADR-0103` there records the presentation layer and terminal face. Both renumbers are address-only; nothing in the Goal, Core, Appetite, or Acceptance moved. The sync the paragraph above anticipated is what carried this slice across.
