# 032 — A build roots itself

## Goal

Nothing pins a project's build. [`../../../reference/spec/11-generations-and-build-history.md`](../../../reference/spec/11-generations-and-build-history.md) holds each retained build in a per-project Nix profile whose numbered symlinks are garbage-collector roots, and no profile exists: `last-build` is a text file holding a store path, which nothing in Nix follows. Measured on this repository's own guest image on 2026-08-14, `nix-store -q --roots` returned nothing and the closure was in the dead set, so an ordinary `nix-collect-garbage` would reclaim 4.5 GiB that a start otherwise reuses in about twenty seconds. Nothing announces the loss, and the next `viv start` succeeds — so a collected store is read as a slow day. After this slice a build is reachable from a root, a user can see what is retained, and reclaiming is something they ask for.

## Appetite

3 implementation sessions.

## Core

Every retained build is reachable from a garbage-collector root, `viv generations list` names what is retained, and unlinking is a verb rather than a side effect. The one outcome the core forbids is a symlink that looks like a root and is not — which is exactly today's failure wearing a new path, and the reason acceptance reads `nix-store -q --roots` rather than the filesystem.

## In scope

Ordered, because retention has to exist before anything lists, unlinks, or reclaims it.

1. Create the profile and prove it roots. Each successful build appends a numbered generation under the state root at the layout [`../../../reference/spec/11-generations-and-build-history.md`](../../../reference/spec/11-generations-and-build-history.md) fixes, keyed on the manifest under [`ADR-0107`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md). Proving it is the item, not a check on it: a symlink under a state directory is a symlink, and what makes it a root is the registration Nix follows. This item is done when `nix-store -q --roots` names the generation, on a real host, and not when the link exists.
2. Retain what reproduces the build beside it. The metadata record and the lockfile snapshot spec/11 enumerates — store path, lock digest, manifest, backend, `built_at`, and the lock that was in force rather than a revision from it, which is [`ADR-0059`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md)'s amendment to [`ADR-0014`](../../../decisions/ADR-0014-build-generations-and-gc-roots.md).
3. Reconcile the two records that already exist. [`../../../../src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs) writes `last-build` and `running-build` as plain paths under the data root, and its own comment says they are deliberately not generations because that scope belonged to a later slice. This is that slice. Either both become readers of the profile, or the freshness record and the retention record are stated as different things in [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) with the reason each lives under the root it does. What does not survive this item is two independent answers to "what did this project last build".
4. Show it. `viv generations list` renders number, timestamp, store path, and lock digest in both the human and JSON renderings, through [`../../../../src/cli/render.rs`](../../../../src/cli/render.rs) like every other data command.
5. Make `viv destroy` unlink. It already removes the build records and spec/10 has it unlink the roots; that clause is structurally a no-op today because there are no roots, and item 1 is what turns it into work. The removal stays inside the teardown boundary [`ADR-0080`](../../../decisions/ADR-0080-the-sandbox-is-disposable.md) fixes.
6. Add `viv generations prune`, with the running-VM guard spec/11 states: a generation the running guest was booted from refuses at `75` and the guard reads the boot record rather than `current`, because `current` may have moved past what is running.
7. Take the exit. [`Q-023`](../../open-questions.md) is resolved by item 1 and recorded as such, with the measured cost it carries preserved where it is still true.
8. Move the `viv generations`, `viv gc`, and `viv destroy` rows in [`../../../reference/implementation-status.md`](../../../reference/implementation-status.md), including the sentence that records the unlink as structurally a no-op.

## Out of scope

- A retention policy that runs by itself. Nothing prunes on a schedule, on a threshold, or at the end of a start; the charter's no-go on background arbitration covers reclaiming as much as it covers memory.
- Deciding how many generations are enough. `--keep` and `--older-than` are the user's levers and vivarium ships no opinion behind them.
- The store-pressure work of Phase 3. What happens when a store fills mid-build is a different cluster and this slice neither consumes nor anticipates it.
- Rooting anything a running guest reads beyond its own build. [`ADR-0085`](../../../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) already fixes that, and item 6's guard is where the two meet.
- Ordered remainder, cut first when the appetite binds: `viv gc`'s whole-store sweep, then `viv generations activate` and `rollback`, then `viv start --generation <n>`. Each is a consumer of the retention items 1 and 2 establish, so cutting one leaves nothing half-built — and `--no-rebuild`, which the grammar already carries, keeps working through item 3 either way.

## Governed by

- [`../../../reference/spec/11-generations-and-build-history.md`](../../../reference/spec/11-generations-and-build-history.md) — fixes the profile, the on-disk layout, the per-generation record, the whole `generations` family, and the separation of unlinking from reclaiming. This slice enacts a page rather than writing one.
- [`../../../decisions/ADR-0014-build-generations-and-gc-roots.md`](../../../decisions/ADR-0014-build-generations-and-gc-roots.md) — chose the per-project profile over bare `result` links and over a tool-managed index with no roots, which is the decision item 1 realizes.
- [`../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md`](../../../decisions/ADR-0059-lockfile-is-tool-owned-in-the-data-root.md) — amended `ADR-0014` so a generation retains the lock rather than a revision; item 2 carries that amendment.
- [`../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md`](../../../decisions/ADR-0107-the-sandbox-keys-on-the-manifest.md) — fixes the key the profile path is built from.
- [`../../../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md`](../../../decisions/ADR-0085-a-running-guest-pins-the-store-paths-it-reads.md) — fixes why unlinking under a live VM is refused rather than merely discouraged, which is item 6's guard.
- [`../../../decisions/ADR-0080-the-sandbox-is-disposable.md`](../../../decisions/ADR-0080-the-sandbox-is-disposable.md) — fixes the teardown boundary item 5's unlink stays inside.
- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — owns the state and data roots item 3 reconciles the two records across.
- [`../../../reference/spec/10-vm-lifecycle.md`](../../../reference/spec/10-vm-lifecycle.md) — has `destroy` unlink the retained builds, which item 5 makes real work.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — assigns `75` to the VM-state precondition item 6's guard answers with.
- [`../../open-questions.md`](../../open-questions.md) — carries `Q-023`, which item 7 consumes.

## Acceptance

After a successful `viv start`, `nix-store -q --roots` SHALL name the generation symlink among the roots of that project's build output, and the assertion SHALL be made against the store rather than against the filesystem, because a symlink that is not registered is the exact defect this slice exists to remove.

After that same start, a store collection SHALL leave the build intact, and the trial SHALL demonstrate it by collecting rather than by asserting the root exists — one clean run is not the claim here, so the observation SHALL be repeated.

`viv generations list` SHALL name every retained generation for the selected manifest in the human and JSON renderings, with the lock digest that makes two of them comparable.

`viv generations prune` SHALL unlink exactly the generations its retention argument selects, SHALL refuse at `75` for the generation a running guest was booted from, and SHALL make that decision from the boot record rather than from `current`.

`viv destroy` SHALL leave no generation of the destroyed project reachable from any root, and `viv gc` — if it survives the appetite — SHALL reclaim only what no root pins.

Whatever item 3 decides, exactly one record SHALL answer what a project last built, and the page that owns the roots SHALL say which record that is and where it lives.

## Rabbit holes

- Creating the symlink and calling it a root — escape: acceptance reads `nix-store -q --roots`, and the 2026-08-14 measurement in `Q-023` is what a project with a link and no root already looks like.
- Building the whole `generations` family because the page specifies it — escape: the ordered remainder puts `activate`, `rollback`, and `--generation <n>` last precisely so the appetite cuts them rather than cutting retention.
- Making a start prune so the disk stops growing — escape: unlinking and reclaiming are separate steps by decision, and a start that reclaims is a start that can delete the thing another sandbox is about to reuse.
- Rooting every intermediate output because the closure is what actually costs — escape: a generation pins its build output and the closure follows from it; a second pinning scheme is bookkeeping `ADR-0014` chose the profile to avoid.
- Reaching for the hand-added `gcroots` workaround because it is what the developer host already has — escape: slice 015 found exactly that link, removed it, and recorded it as the workaround `Q-023` anticipated. A product that needs it has not shipped this slice.

## Done when

Every acceptance assertion above holds and is demonstrated by the trials that name it, the repeated collection observation is recorded with its figures in [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md), `Q-023` is closed and removed with its chosen exit recorded where that exit belongs, item 3's reconciliation is written in [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) rather than only in this document, the rows item 8 names are moved, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-25, before any work started, from the same owner review that shaped [slice 031](../031-a-declared-agent-channel-reaches-the-guest/README.md). It was ranked second of the four gaps priced there on a single argument: every other gap costs a user a feature they do not have, and this one costs them a build they did have, silently, at a price — roughly twenty-nine minutes against twenty seconds — that is paid long after the act that caused it.

The cluster this slice consumes sat in [`../../sequencing.md`](../../sequencing.md) Phase 3, decided and unfunded, since before slice 011. What moved it is not a new decision but a measurement: slice 015's host sweep found the current build reachable from no root at all, and found the hand-added `gcroots` link a developer had made to work around it. A decision that has been enacted by hand on the only host that runs the product is funded whether or not the plan says so.

The appetite is three sessions and the remainder is ordered so that retention survives it. That ordering is the whole shape of the slice: `viv gc` is the verb a reader expects to be the point, and it is last, because a sweep with nothing rooted against it is more dangerous than no sweep at all.
