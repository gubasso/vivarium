# 030 — The base image is the base

## Goal

Make the boot lanes boot again, and name the artifact they boot for what it is: the base image every verification variant extends, with a check that keeps the extending honest.

## Appetite

5 implementation sessions.

## Core

The four host lanes that invoke a launcher actually reach a running guest, `packages.base-image` is the shipped artifact, and a verification image that replaces rather than appends fails at `nix flake check`.

## In scope

Ordered, because the rename moves the files the later items edit.

1. Rename the artifact. `first-microvm` becomes `base-image` across [`../../../../nix/`](../../../../nix), [`../../../../tests/nix/`](../../../../tests/nix), the host lanes, and the prose that names them. The launcher binary becomes `vivarium-base-image` and the lane becomes [`../../../../tests/host/base-image-check`](../../../../tests/host/base-image-check). Frozen decision bodies and closed slice records keep the name they recorded; only living documents move.
2. Assert the extension. A new evaluation-tier check compares each verification image's resolved settings and guest configuration against the base image's, and fails when a variant varies on a key it did not declare. This is what `ADR-0111` buys and the reason the rename is worth its diff.
3. Repair the four dead lanes. `base-image-check`, [`../../../../tests/host/store-pressure-check`](../../../../tests/host/store-pressure-check), [`../../../../tests/host/store-gc-interlock-check`](../../../../tests/host/store-gc-interlock-check), and [`../../../../tests/host/share-benchmark-check`](../../../../tests/host/share-benchmark-check) each invoke the runner without `--supervisor` and then wait for a console socket, but since `ADR-0102` the runner renders the specification and execs nothing. Each gains the two-step boot [`../../../../tests/guest_agent_host.rs`](../../../../tests/guest_agent_host.rs) already performs: render with the runner, boot with `viv start --spec`.
4. Assert the shipped package's own rendering. `--mount` is optional and the base image declares none, so the three image-independent facts in [`../../../../tests/host/exec-and-shell-check`](../../../../tests/host/exec-and-shell-check) move onto the base image, and only the share-mirroring fact stays on the image that declares a mount.
5. Account for what an older version left. [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) names one legacy state-root file; it names the set instead, including the identity index, both lock files, and the retained project root the `state-manifest-orphans` probe reports.
6. Take the exits. [`../../open-questions.md`](../../open-questions.md) `Q-030` is resolved by item 3 and recorded as such.
7. Record what the derived index's atomicity rests on. [`../../../../src/config/registry.rs`](../../../../src/config/registry.rs) argues lockless publication in a module comment; the argument, and the stray-sibling case it does not cover, belong where the design lives.

## Out of scope

- The measurement legs themselves. What each probe measures is unchanged; only how the lane reaches a booted guest moves.
- The launch contract. No schema version changes, no argument is added to `nix/runner.sh`, and `--supervisor` was always required — the lanes simply never passed it.
- Retiring any lane. `Q-030` offers retirement as an exit and this slice declines it: the four lanes measure memory return, real-filesystem collection, and virtiofsd pool behaviour, none of which the acceptance trials cover.
- Renaming anything inside a frozen decision body.
- Ordered remainder, cut first when the appetite binds: item 7, then item 5.

## Governed by

- [`../../../decisions/ADR-0111-verification-images-extend-the-base-image.md`](../../../decisions/ADR-0111-verification-images-extend-the-base-image.md) — the decision items 1 and 2 enact.
- [`../../../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) — ended the runner's job at rendering, which is the defect item 3 repairs.
- [`../../../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md`](../../../decisions/ADR-0095-measurement-services-live-in-a-measurement-image.md) — put the probes in a variant, which is what item 2 keeps honest.
- [`../../../decisions/ADR-0098-product-and-verification-have-one-way-repository-boundaries.md`](../../../decisions/ADR-0098-product-and-verification-have-one-way-repository-boundaries.md) — fixes the direction item 2's new check must not reverse.
- [`../../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md`](../../../decisions/ADR-0054-stale-bindings-surfaced-not-reaped.md) — owns the report-rather-than-reap judgement item 5 documents.
- [`../../../reference/spec/02-config-and-xdg-layout.md`](../../../reference/spec/02-config-and-xdg-layout.md) — the page item 5 edits.
- [`../../../reference/microvm-verification-harness.md`](../../../reference/microvm-verification-harness.md) — the operator-facing record of what each lane proves, which items 1 and 3 move.
- [`../../../reference/testing-lanes.md`](../../../reference/testing-lanes.md) — names the lanes item 1 renames and item 3 repairs.

## Acceptance

Each of the four repaired lanes SHALL reach a booted guest and report its own checks, rather than exiting at a usage line. A run SHALL be repeated, because one clean run is evidence of possibility and not of reliability.

A verification image that varies a setting it did not declare SHALL fail `nix flake check` with a message naming the image and the key.

`viv config eval`, the launch contract, and every published diagnostic id SHALL be unchanged: this slice renames an artifact and repairs a harness, and a user SHALL observe no difference in the tool.

## Rabbit holes

- Rewriting the lanes around `viv start` rather than around the two-step render-then-start. The lanes need the rendered specification for their own assertions, and `viv start --spec` is the seam that keeps both halves; going through `viv start` alone would delete the rendering evidence.
- Making the extension check compare whole guest configurations. A NixOS configuration is not comparable attribute by attribute at reasonable cost, and a check that throws on `_module` internals fails for its own reasons. Compare the resolved settings, the module list, and the share and volume lists.
- Building the supervisor from the lanes in a way that diverges from how `viv` resolves it. The lanes take the same binary the acceptance trials take.
- Renaming inside frozen decision bodies. The name a record recorded stays recorded.

## Done when

The four lanes boot, twice. `nix flake check` carries the extension check and it fails on a deliberately replicating variant. `just check` and `just test-pre-push` pass with no skips. `grep -rn first-microvm` returns only frozen decision bodies and closed slice records. `Q-030` names its exit, and `docs/plan/milestones.md` carries `030` as `done`.

## Revisions

2026-08-24 — the scope of item 3 was wrong when it was written, and the correction is the slice's main finding. `Q-030` named one lane; the sweep found four. `base-image-check`, `store-pressure-check`, `store-gc-interlock-check` and `share-benchmark-check` all invoked the runner without `--supervisor` and then waited for a console socket the runner never creates, so every check behind those blocks had been reporting against a guest that never booted since `ADR-0102`. Confirmed against the built binary rather than inferred: that argument set answers with the usage line and exit `64`. No appetite change — the repair is the two-step launch `tests/guest_agent_host.rs` already performed, applied four times.

2026-08-24 — three checks were found encoding a wrong assumption, all of them invisible while the lanes could not reach them, and each corrected with its reason recorded as [slice 010](../010-repair-the-host-runbooks/README.md)'s acceptance requires. The confinement contract demanded a `uid_map` of exactly `<uid> <uid> 1`, but `src/net/netns.rs` builds the VMM's network pair with `--map-root-user` so cloud-hypervisor's map reads `0 <uid> 1` — one host identity wide, which is the property that confines; the check now compares the host side and the length. `share-bench-pool-sizes-distinct` grepped for a store derivation named `vivarium-<image>-launch-arguments.json`, which has never existed, so it compared an empty string against every pool size; it now reads the published `share/vivarium/launch-arguments.json` the sibling lanes already read. The third is below, because it is a coverage change rather than a repair.

2026-08-24 — `base-image-check` can no longer assert N16, and that is a consequence of `ADR-0110` rather than of this slice. The lane compared the guest's bind target against this repository's own path; `ADR-0110` made the bind target build output, and a verification image's declared tree is the synthetic `/VIVARIUM_VERIFICATION_WORKSPACE` because a statically-evaluated image cannot know the directory a checkout occupies. Host symmetry is therefore a claim only a real manifest can make, and `workflow_09_round_trip` makes it by comparing the guest session's own `pwd` against the project path. The lane now asserts what is in reach — that the bind unit and the kernel agree, on the target the build publishes — with the moved coverage named at the call site and in the harness register rather than left to be inferred. No open question is raised, because the coverage exists and was verified to exist.

2026-08-24 — item 4 was resolved as a settled no rather than an open question, which is what the audit that seeded this slice asked for. `--mount` is optional and the runner compares an empty declared set against an empty supplied set, so a zero-mount rendering of the base image was available the whole time; the premise that the shipped package could not be rendered was half right, and the half that was wrong is the half the comment rested on. `exec-and-shell-check` now asserts the lock path, the session user and the session PATH against the shipped artifact, keeps the source assertion on the image that declares a tree, and asserts the zero-mount premise itself so the split fails loudly if the base image ever gains a declared share.

2026-08-24 — the extension check found its first subject while being written. `images.agent` did not go through `mkVerification` at all: it called `product.mkImage` directly and hand-copied the tree block every other verification image shares, which is the replication `ADR-0111` exists to prevent, present in the file that defines the rule. It now goes through `mkVerification` like its siblings. The check was verified by two deliberate negative controls before being trusted — a variant that force-drops `microvm.shares` and `systemd.services`, and one whose declared-override set is emptied — and both fail with the image and the dropped names printed. A check that has never been shown failing is not evidence.

2026-08-24 — host evidence, on the capable host, drive `/run/media/gbasso/king-silver/vivarium`, every heavy command through `tests/host/heavy-run`. `just check` 296/296 with one skip. `nix flake check` all five checks including the new `base-image-extends`. `base-image-check` `PASS=42 FAIL=0 SKIP=0`, `store-gc-interlock-check` 17/0/0, `store-pressure-check` 21/0/3, `share-benchmark-check` 18/0/0 with all four pools booting, `exec-and-shell-check` 5/0/0. Each of the four repaired lanes was then run a second time and reported the same verdict, because one clean run is evidence of possibility and not of reliability. `just test-pre-push` 55/55 with zero skips, twice, under `VIVARIUM_TEST_REQUIRE=1` and a realized `VIVARIUM_AGENT_RUNNER`. The complete pre-commit hook set passed. `tests/host/leftovers` reported the drive carried the round with nothing belonging to it on the boot disk. One `nix flake check` invocation was refused by the disk preflight at a plan-time `--need 40` against a store with 35G free; the requirement was re-measured with `nix build --dry-run`, which showed nothing left to build, and re-declared at `5` rather than waived with `--on-host`.

2026-08-24 — five review rounds, and the shape of what they found is worth recording because it is not the shape the slice expected. Round 1 found the extension check comparing shares, volumes and services by identifier alone, so a module could `lib.mkForce` a base entry's content while keeping its name and pass — the check was weaker than `ADR-0111` claimed. Rounds 2 through 4 then found four successive defects in the repair of that and of the lane cleanup, each one a smaller version of the last: excluding a volume's `size` unconditionally, then excusing it whenever the key was declared rather than requiring the resolved size to equal the declared one, then reading `systemctl is-active` as "stopped" when it reports failure the moment a unit enters `deactivating`, then keeping ownership in a scalar that a four-pool sweep overwrites. Round 5 approved with no findings.

The common thread is one mistake in four costumes: asserting a property against a proxy for it rather than against the property. An identifier proxies for an entry, a declared setting proxies for a resolved value, `is-active` proxies for "finished stopping", and a unit name proxies for a unit this run created. Each proxy was right in the common case and wrong in exactly the case the check existed to catch. The slice's own `Rabbit holes` warned about the first of these and the warning did not generalise.

Two consequences beyond the fixes. The extension check's honest limit is now stated rather than implied: unit CONTENT is not compared, because every variant legitimately changes some unit — `vivarium-agent` in all of them, `nix-daemon` under declared store thresholds — so a content comparison would fail on every image for its own reasons. `ADR-0111` carries that as a `Bad` consequence rather than letting its `Good` one over-claim. And the two-step launch introduced a real cleanup regression that had no analogue in the old shape: `viv start --spec` submits the unit and then waits for readiness, so a failure between the two can leave a VM serving a tree the lane is about to delete, where the old backgrounded launcher had already exited having booted nothing. Every lane now claims its unit name before submitting, remembers what it claimed, stops only what it claimed, waits for a terminal state rather than for departure from `active`, and retains the scratch tree when a teardown does not settle.

Six negative controls were run across the rounds, because a check never shown failing is not evidence: force-dropping shares and services, emptying the declared-key set, force-replacing a base share's `socket`, force-replacing a base volume's `image`, force-changing a size with no key declared, and force-changing a size to a value the declared key contradicts. All six fail with the image and the entry named.

2026-08-24 — the legacy state-root files item 5 documents were also removed from this host: `identity.toml`, `registry.toml` and both `.lock` files, plus five empty pre-drive lane directories. The retained `projects/vivarium/` root was left in place at 531 MB. It is a genuine orphan — the manifest library holds `agent` and `rust-web`, and nothing is named `vivarium` — but `ADR-0054` makes removing it an operator decision, and `tests/host/leftovers` classifies it as "not leftovers" and points at `viv destroy` rather than at `rm`.
