# 022 — A file mount serves only its file

## Goal

Close the confinement gap slice 019 shipped with a recorded residual: a declared regular-file mount is served through its parent directory, and guest root can remount the share's tag and reach every sibling of the declared file — writable when the declaration is not `readonly`. After this slice the exposure is impossible rather than advised against: either the export carries only the declared file, or the declarable surface is narrowed so the exposing spellings refuse before boot.

## Appetite

2 sessions.

## Core

A guest — including guest root, remounting the share's virtiofs tag — cannot reach any entry of a file mount's parent directory other than the declared file, demonstrated by an adversarial trial; or, where every priced export design fails, an ADR records and enacts the narrower declaration rule that makes the exposure undeclarable, with the refused spellings failing before boot under a named code. The one outcome the core forbids is the current state: a residual paragraph and a SHOULD.

## In scope

Ordered, because the decision cannot be recorded before the candidates are priced, and nothing can be enacted before it is decided.

1. Price the export-level candidates by measurement against the pinned virtiofsd and this host, not by reading: a supervisor-prepared staging directory holding a bind mount of the single file (unprivileged mount namespaces, `open_tree`/`move_mount`, and whether the staged mount is visible to the daemon's own namespace under the N20 profile); the pinned daemon's own surface (submount and allowlist options, Landlock on the daemon process); a hard link into a staging directory (expected to fail across filesystems — confirm and record); a copy with write-back (breaks live updates — price whether the identity-file use case tolerates it). The question this consumed carried the two failures already known.
2. Record the decision as an ADR: the chosen shape, and each rejected candidate with its measured reason.
3. Enact it. Either the export restriction in the launch path — the staging preparation in [`../../../../src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs) or [`../../../../src/launch/supervisor.rs`](../../../../src/launch/supervisor.rs), whichever side the design puts it on — or the declaration narrowing in the same refusal tiers slice 019 built: the merged view at `65` for what text decides, [`../../../../src/launch/mounts.rs`](../../../../src/launch/mounts.rs) at `78` for what expansion decides.
4. Land the adversarial trial: a sibling planted beside the declared source, and required absent from the share's mount point in the guest and from the share daemon's own root on the host. The slice 019 trials (`workflow_17_declared_mounts_round_trip` among them) keep passing unchanged, or their `Revisions`-recorded change is part of item 3's enactment.
5. Rewrite the accepted-residual paragraph in [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) into the guarantee (or the narrowed rule) the decision gives, and close the open question this slice consumed.

## Out of scope

- Directory-mount confinement. A directory source's whole tree is the declaration; nothing narrows there.
- Mirroring a mount at its own host path ([slice 020](../020-many-workspaces-in-one-sandbox/README.md)) and sandbox identity ([slice 021](../021-the-manifest-is-the-sandbox/README.md)), both sequenced behind this.
- Upstream virtiofsd contributions. Item 1 prices what the pinned daemon can do; a missing upstream feature becomes a candidate exit for the ADR, not work here.
- Ordered remainder, cut first when the appetite binds: the copy-with-write-back pricing beyond a recorded paragraph, and a `viv doctor` lint advising a dedicated subdirectory while any advisory rule remains.

## Governed by

- [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) — carries the accepted residual this slice deletes, and the per-share confinement the fix must keep.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — owns the code any new refusal answers with.
- [`../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md`](../../../decisions/ADR-0020-mount-and-config-mirroring-schema.md) — fixes the declaration schema a narrowing would amend.
- [`../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md`](../../../decisions/ADR-0071-agent-forwarding-over-a-second-vsock-port.md) — fixes what a source may be.
- [`../../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md`](../../../decisions/ADR-0027-vmm-and-virtiofsd-hardening-launch-profile.md) — fixes the N20 profile any staging design must run inside.

## Acceptance

When a regular-file mount the decision admits is declared, no entry of the source's parent directory other than the declared file SHALL be reachable through that share by any guest process, guest root included. A trial SHALL demonstrate this on a booted guest at the boundary that bounds guest root — the share daemon's own root directory, read from the host while the guest runs — and from inside the guest at the share's mount point, with a sibling of the declared source planted on purpose and absent from both.

If the decision narrows the declarable surface instead, each refused spelling SHALL fail before boot with the code the ADR assigns, a trial SHALL demonstrate the refusal, and the admitted spellings SHALL keep the slice 019 acceptance passing.

The decision SHALL be recorded as an ADR whose rejected candidates each carry a measured reason, and [`../../../reference/spec/06-workspace-and-project-environment.md`](../../../reference/spec/06-workspace-and-project-environment.md) SHALL state the resulting guarantee in place of the accepted residual.

## Rabbit holes

- Growing a per-share policy engine because one share kind needs a restriction — escape: this slice constrains exactly the regular-file case; everything else keeps slice 019's shape.
- Hardening the guest-side shadow further — escape: measured in slice 019's review, guest root defeats any guest-side masking; the fix is host-side or declarative, never another guest mount.
- Waiting on upstream — escape: the ADR decides from what the pinned daemon does today; an upstream feature is a recorded future exit, not a dependency.
- Solving live write-back for copies in general — escape: pricing it is item 1's paragraph; building a sync engine is not in any candidate this slice may choose.

## Done when

Every acceptance assertion above holds and is demonstrated by the trial it names, the decision ADR is `Implemented`, the open question it consumed is closed and removed, the spec/06 residual paragraph is replaced, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

Recorded 2026-08-18, at shaping, before any work started. Slice 019's review loop found the exposure — the guest-side tmpfs shadow bounds ordinary guest processes but not guest root, and no guest-side masking can shrink what the daemon exports — and the accepted-and-recorded close was chosen to keep that slice inside its appetite, with this slice named the next to start so the residual is an ordered piece of work rather than a standing waiver.

Closed 2026-08-18 in one merged pass under the appetite. The pricing in item 1 resolved by measurement rather than by reading, and two of its results shaped the enactment: the pinned daemon starts inside a namespace someone else prepared and pivots into it, which made the staging design possible; and the descriptor forms of the new mount API return `EINVAL` on this kernel, which removed the only reason to prefer `open_tree` over `MS_BIND`. A third result was a defect this slice would otherwise have shipped: a read-only remount that omits `nosuid`, `nodev` and `noexec` is refused when the source lives under a filesystem carrying those flags locked, so the failure reproduces on one host and not another. All three are in [`the harness findings`](../../../reference/microvm-verification-harness.md), and the trial was run against a build with the staging removed, where it fails naming the planted sibling.

Acceptance reshaped 2026-08-18, at the start of the work, before the enactment landed. The original wording asked a trial to attack from inside the guest as root; the product hands out no root in the guest — `viv exec` runs as the session user, the agent unit runs as `vivarium`, the image carries no `sudo` and no getty, and a piece sets only `vivarium.*` options — so no trial can reach that state through the product surface, and a trial that could would be asserting on a seam built for the trial. The demonstration moved to where the property is enforced instead: the share daemon's own root directory, which bounds every guest process including root, read from the host while the guest runs. The negative control is recorded below rather than assumed.
