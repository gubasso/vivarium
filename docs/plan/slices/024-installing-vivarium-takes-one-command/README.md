# 024 — Installing vivarium takes one command

## Goal

vivarium has never been released. [`../../../../Cargo.toml`](../../../../Cargo.toml) still reads `0.1.0`, no tag exists, and the release path [`../../../guides/publishing.md`](../../../guides/publishing.md) describes has never run once, so the only way to obtain the tool is to clone this repository and build it. After this slice a version is on crates.io, the tag-driven release runs unattended, a user installs `viv` in one command from the channel their machine already uses, and the tool itself names what the host still owes before a sandbox can boot.

## Appetite

5 sessions.

## Core

A named version of vivarium is installable by someone who has never seen this repository, through crates.io and through at least one package channel per major host family the tool can actually run on, and the path from a merged release PR to every artifact runs with no hand-made tag and no hand-uploaded file. The one outcome the core forbids is a package that installs cleanly and leaves the user to discover the host prerequisites by watching a command fail.

## In scope

Ordered, cheapest channel first, because each later one consumes the artifacts the earlier ones prove. Cut from the bottom.

1. Exercise the existing pipeline end to end and repair what its first real run breaks: the release-plz PR, the crates.io publish over OIDC, the App-token tag push, `promote-to-master.yml`, and the cargo-dist build. Verify the packaged tarball against the `exclude` list `publishing.md` prescribes before the version is spent, because a published version cannot be retracted. Settle the target list in [`../../../../dist-workspace.toml`](../../../../dist-workspace.toml) in the same pass: it builds `aarch64-apple-darwin`, `x86_64-apple-darwin`, and `x86_64-pc-windows-msvc` for a tool that requires a Linux host with `/dev/kvm`, so today's release would attach binaries that cannot run. Either the targets narrow to what the product supports or the reason for shipping them is written down.
2. Cut the first release and record its number.
3. Decide the channel set as an ADR: which channels ship, what each package contains, and the dependency posture. The posture is the load-bearing half — a package that installs `viv` without Nix has moved the failure later rather than removed it, so the ADR says for each channel whether it declares a dependency on that distribution's Nix, and the rejected shape carries the reason that rejected it. It also decides whether [`../../../../nix/flake.nix`](../../../../nix/flake.nix) publishes a `viv` package, which is a question against [`../../../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) rather than a packaging convenience: that record removed the tool as an input to a generated project flake, and a Nix-installable tool is a different thing wearing a similar shape.
4. Take the channels that ride the cargo-dist artifacts already produced, since these cost configuration rather than infrastructure: the Homebrew tap installer `dist` generates but `dist-workspace.toml` does not currently enable, and an AUR package built from the tagged release rather than from source. Confirm `cargo-binstall` works from the release as `publishing.md` claims, rather than restating the claim.
5. Take the Nix channel the ADR chose, and prove the install command a reader would run.
6. Take the channel that needs its own source form: an OBS build needs a vendored tarball produced by the release, because an OBS worker has no network and resolving crates at build time is the failure AGENTS.md already names. Land the `.spec` and `_service`, and prove the install on a clean host of the targeted distribution — package installed, `viv --version` correct, `viv doctor` run.
7. Make `viv doctor` answer the prerequisite question for a packaged install: a host carrying the package and lacking Nix, or lacking `/dev/kvm`, meets a named diagnostic saying what is missing and what to do, under the catalog [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) owns. This item does not survive a cut. It is the difference between winning the row and appearing to, and it is what makes cutting any channel below safe.
8. Write the operator-facing half: extend `publishing.md` with each channel and its cadence, give a reader one install page naming the channels and the prerequisites, and add the perishable packaging assumptions to [`../../../reference/tracking.yaml`](../../../reference/tracking.yaml) beside the release entries already there.

## Out of scope

- Packaging the guest image, the product Nix tree, or anything that is not the host tool and its supervisor.
- Bundling, vendoring, or installing Nix on the user's behalf. The prerequisite is named, not absorbed.
- A stable-API promise. The crate is binary-only, `publishing.md` fixes the SemVer posture, and this slice releases under it.
- Windows and macOS support. If item 1 narrows the target list, that is a correction to what is claimed, not a decision to drop a platform that worked.
- Ordered remainder, cut first when the appetite binds: upstreaming to `nixpkgs`, whose acceptance is an external review this repository cannot schedule; a second distribution family beside the first in OBS; and man pages and shell completions in the packages.

## Governed by

- [`../../../guides/publishing.md`](../../../guides/publishing.md) — owns the release path this slice runs for the first time and extends with the packaging channels.
- [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) — owns the catalog the prerequisite diagnostic joins.
- [`../../../reference/spec/00-goals-and-non-goals.md`](../../../reference/spec/00-goals-and-non-goals.md) — owns whether a distribution channel is a product goal at all.
- [`../../../decisions/ADR-0102-the-installation-supplies-vivarium.md`](../../../decisions/ADR-0102-the-installation-supplies-vivarium.md) — fixes that the installation supplies vivarium, which is what makes how it is installed a product question, and bounds the Nix-package question in item 3.
- [`../015-the-binary-supplies-itself/README.md`](../015-the-binary-supplies-itself/README.md) — parked release packaging and installation channels in its `Out of scope`; this slice is where that deferral lands.
- [`../../../reference/comparison/scenarios/install.md`](../../../reference/comparison/scenarios/install.md) — the dated measurement that raised this slice.
- [`../../../reference/tracking.yaml`](../../../reference/tracking.yaml) — owns the perishable release facts this slice adds to.

## Acceptance

A version of vivarium SHALL be resolvable from crates.io and installable from a package on a host that has never held this repository, and each channel SHALL be demonstrated by an install on a clean host rather than asserted from its configuration.

Merging a release PR SHALL produce the tag, the crates.io publish, the `master` fast-forward, and every attached artifact, with no hand-made tag and no hand-uploaded file.

Every binary artifact a release attaches SHALL be for a platform the product supports, or the reason for attaching it SHALL be recorded.

The source an OBS build consumes SHALL build with no network access from the build worker.

A host carrying the package and lacking Nix or `/dev/kvm` SHALL meet a named `viv doctor` diagnostic stating what is missing and what to do, and this SHALL be demonstrated on such a host rather than assumed.

The channel set and the per-channel dependency posture SHALL be recorded as an ADR whose rejected shape carries the reason that rejected it.

## Rabbit holes

- Taking every channel because the mechanism generalises — escape: the ADR names the set, the items are ordered by cost, and the appetite cuts from the bottom.
- Making a package carry Nix so the install is self-sufficient — escape: out of scope above. A named prerequisite is a smaller lie than a package that pretends there is none.
- Rewriting the release pipeline because its first real run failed — escape: item 1 repairs what breaks. A pipeline that has never run is expected to break, and that is not evidence its shape is wrong.
- Spending versions on debugging — escape: `publish-dry` and `cargo package --list` before the tag; a published version cannot be taken back, and a burnt one is permanent.
- Waiting on `nixpkgs` review — escape: it is ordered remainder precisely because its clock belongs to other people.
- Rewriting the comparison row to say the gap is planned — escape: that row is dated measurement. It changes when a later sweep measures a shipped package, not when a slice is shaped.

## Done when

Every acceptance assertion holds and is demonstrated on the host it names, the channel ADR is `Implemented`, `publishing.md` covers every shipped channel, the doctor catalog carries the prerequisite diagnostic, `tracking.yaml` carries the new perishable facts, and the [`../../milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Shaped 2026-08-20, before any work started, from the `Installs from a distro package` row of the comparison, which reads no for vivarium and yes for `flake-pilot`, `glaipnir`, and `podman`. Slice 015 had already parked release packaging in `Out of scope` with no successor, and this slice is that successor.

Widened at shaping time, before landing, from one distribution target to a cost-ordered channel set. The first shape proved the mechanism once and left the rest to a later slice; the owner asked for the cheap channels that ride artifacts the release already produces, so the ordering carries the appetite instead. The prerequisite diagnostic moved above the channels in the same pass and is marked non-cuttable, which is what lets any channel below it be cut without the slice shipping a false promise.
