# Sequencing

A decision record says whether a choice was made. It never says when that choice gets built, and its [`## Status`](../decisions/template.md) vocabulary has no word for waiting. That is deliberate: status is frozen from `Proposed` onward, so sequencing cannot live in the decision. It lives here.

This page maps decision clusters onto the slices that consume them. It names clusters and their consuming slice only; what each decision decided stays in the decision, and [`../decisions/`](../decisions/) owns the inventory. Status stays in [`milestones.md`](./milestones.md).

Read it to answer one question: a decision exists, so when does it become code?

## Standing constraints

These bound every phase and no slice re-decides them, so they are not sequenced. `ADR-0001` fixes the isolation boundary, `ADR-0008` the two-layer separation, `ADR-0010` that secrets never enter the store, `ADR-0024` the backend security requirements, and `ADR-0025` the default hypervisor. A slice that appears to need one of these relaxed has found a charter question, not a sequencing one.

A decision consumed by a closed slice is already realized; [`../reference/implementation-status.md`](../reference/implementation-status.md) records how far. `ADR-0016`, `ADR-0065`, and `ADR-0071` appeared below through slice 013 for the opposite reason to the others: their guest half was realized a slice before their host half.

## Phase 1 — a sandbox that runs

The chain in flight. These clusters are consumed by slices 011 through 014, and each slice's `Governed by` section carries the individual links.

- Manifest, composition, and artifact model — `ADR-0002`, `ADR-0003`, `ADR-0004`, `ADR-0061`. Consumed by [slice 011](slices/011-resolve-and-evaluate/README.md).
- Config roots, binding, and precedence — `ADR-0005`, `ADR-0006`, `ADR-0011`, `ADR-0026`, `ADR-0029`, `ADR-0045`. Consumed by slice 011.
- State layout and durability — `ADR-0052`, `ADR-0053`, `ADR-0058`, `ADR-0059`. Consumed by slice 011.
- CLI output and failure contract — `ADR-0015`, `ADR-0028`, `ADR-0033`. Consumed by slice 011, with the `start` boundary settled in [slice 012](slices/012-first-boot/README.md) through Q-008.
- Lifecycle and runtime ownership — `ADR-0013`, `ADR-0018`, `ADR-0030`, `ADR-0055`, `ADR-0056`, `ADR-0097`. Consumed by slice 012 for the boot itself, and again by [slice 013](slices/013-exec-and-shell/README.md) for the reuse-or-boot routine a command needing a VM runs.
- Guest store provisioning — `ADR-0038`, `ADR-0084`, `ADR-0087`, `ADR-0088`. Consumed by slice 012.
- Open egress by default — `ADR-0007`. Consumed by slice 012 as the reason a first boot needs no network work.
- Guest control transport — `ADR-0016`, `ADR-0065`, `ADR-0071`. Guest half realized in slice 003; host half realized in [slice 013](slices/013-exec-and-shell/README.md).
- Workspace, volumes, and disposability — `ADR-0009`, `ADR-0017`, `ADR-0019`, `ADR-0037`, `ADR-0043`, `ADR-0066`, `ADR-0067`, `ADR-0080`, `ADR-0092`, `ADR-0100`. Consumed by slice 012 for the mount launch constructs and [slice 014](slices/014-workspace-and-persistence/README.md) for what persists; slice 014 item 2 settled `ADR-0066` and `ADR-0092` as first-boot correctness, so slice 012's `Governed by` now carries both. `ADR-0100` was raised by slice 014 item 1 rather than shaped ahead of it, and supersedes `ADR-0017` in the same slice that consumes it.

## Phase 2 — funded and sequenced behind it

Shaped slices carrying the `later` status. They are not deferred indefinitely; they are sequenced behind a product that runs.

- Egress enforcement — `ADR-0007`, `ADR-0044`, `ADR-0064`. Consumed by [slice 004](slices/004-enforce-egress-allowlist/README.md).
- Self-supply and lock scope — `ADR-0102`, with `ADR-0058` and `ADR-0059` bounding the seam it rides and the pins it leaves alone. Consumed by [slice 015](slices/015-the-binary-supplies-itself/README.md), sequenced ahead of slice 005: the flawed shape `ADR-0102` removes — a generated flake pinning a Nix-built `viv` against the installed one — sits under every later slice's verification, it produced a launch that reported success against a record the running tool could not read, and removing a self-dependency gets more expensive with each slice built on top of it.
- CLI runtime plumbing — `ADR-0031`, `ADR-0032`, `ADR-0033`, `ADR-0034`, `ADR-0069`. Consumed by [slice 005](slices/005-cli-runtime-plumbing/README.md).
- Generated config contract — `ADR-0012`, `ADR-0047`, `ADR-0057`. Consumed by [slice 006](slices/006-generate-config-contract/README.md).
- Build and test lanes — `ADR-0076`, `ADR-0077`. Consumed by [slice 007](slices/007-build-test-lanes/README.md).
- Status enforcement and stability policy — `ADR-0012`, `ADR-0075`. Consumed by [slice 008](slices/008-enforce-implementation-status/README.md).
- Backend advisories — `ADR-0049`, `ADR-0078`, `ADR-0079`. Consumed by [slice 009](slices/009-automate-backend-advisories/README.md).

## Phase 3 — decided, not yet funded

No slice consumes these. Each cluster is a decision made in advance of the work, which is why it is written down rather than remembered. Funding one means shaping a slice for it, at which point its cluster moves to Phase 2.

- Store robustness under pressure — `ADR-0039`, `ADR-0050`, `ADR-0083`, `ADR-0085`, `ADR-0086`, `ADR-0090`, `ADR-0091`.
- Composition extensions beyond one manifest — `ADR-0020`, `ADR-0021`, `ADR-0041`, `ADR-0060`, `ADR-0062`, `ADR-0063`, `ADR-0073`, `ADR-0074`.
- Generations and build history — `ADR-0014`.
- Memory elasticity and capacity — `ADR-0035`, `ADR-0082`, `ADR-0094`.
- Doctor — `ADR-0023`.
- Diagnostic presentation — `ADR-0068`.
- Config inspection and global precedence — `ADR-0022`, `ADR-0042`, `ADR-0046`.
- Sharing and secrets policy — `ADR-0040`.
- Binding hygiene — `ADR-0054`.
- Repository boundaries and best-effort confinement — `ADR-0098`, `ADR-0099`.

## What the acceptance harness is still owed

Phase 1 is being built in the order the acceptance trials already assert, so on a host with `/dev/kvm` part of the harness is red by construction rather than by defect. That distinction only survives if it is written down: a red nobody has mapped to work reads exactly like a regression, and the next person to see it either debugs a verb that was never written or learns to ignore the lane.

Re-measured 2026-08-13 after slice 014, on the same host with `/dev/kvm`, a systemd user manager, and `$XDG_RUNTIME_DIR`: 18 of 19 acceptance trials pass, and the one that does not is waiting on the verb slice 004 implements. `viv volume list`, `viv volume prune`, and `viv destroy` no longer appear in the `Refused by` column at all.

| Trial                                   | Refused by    | Turns green in |
| --------------------------------------- | ------------- | -------------- |
| `workflow_05_restrict_egress_allowlist` | the allowlist | slice 004      |

The remaining red is not a defect and not a partial implementation. `workflow_05` reaches the guest, runs its command, and returns that command's own status — `6`, from a `curl` that could not resolve a `.example` host — which is the session contract working and the enforcement fixture the trial's own `TODO(impl)` records as missing.

That reading was half the story, and the missing half is what slice 004 has to design against. Measured in a booted guest on 2026-08-13, the guest holds no network device: `ip -o link` reports `lo` alone and every name fails identically, so `curl` exits `6` for `api.anthropic.com` exactly as it does for a `.example` host. The `6` therefore says nothing about the allowlist and nothing about DNS — it is what an absent uplink returns for everything. The consequence is that this trial cannot separate a denial from an absence, and it will keep passing its own way once connectivity lands, because a denied name is still a name that fails to resolve. A trial that greens on the day the tap appears would report the network rather than the policy, which is the failure this repository's own harness note warns about. [`Q-022`](./open-questions.md) owns the separation and blocks slice 004's acceptance rather than its contract.

Two things this measurement replaces are worth stating, because a table that only shrinks hides them. Slice 014's own trials went green by implementing three verbs and, underneath them, the declared-volume path the trials assume: `workflow_07_stop_restart_preserving_volumes` asserts an image for a `[[volumes]]` entry, and named volumes had been declarable and inert until this slice attached them. And the binary is 19 trials rather than the 18 the previous measurement counted plus one, because `workflow_09_workspace_round_trip` arrived between the two.

The execution order is the chain already in flight and is not changed by this table: slice 014, then slice 004 out of Phase 2. What the measurement above does change is where slice 014 ends. Its implementation is complete and its last `Done when` clause is a coding-agent run, which needs a guest that can reach a network, so the row closes after slice 004 rather than before it. The two slices do not interleave and neither reopens: 004 starts next either way, and 014 is closed by an observation on the guest 004 delivers. Nothing is added to a slice's `In scope` here; each trial is already named by the item that lands it.

Re-measured 2026-08-14 after slice 004, on the same host: every acceptance trial the harness holds passes, and the table above is empty — no trial waits on unimplemented work. `workflow_05_restrict_egress_allowlist` went green the way [`Q-022`](./open-questions.md) required rather than the way the paragraph above warned against: its fixture serves two `.test` names on two different addresses from inside the VM's own namespace, so the allowed leg is a reachable destination an absent uplink cannot fake, and the denial is read as `REFUSED` at the resolver itself, distinguished from an allowlisted name whose upstream answer is `NXDOMAIN`. The stage the harness held against unimplemented verbs is a real gate again.

### The pre-push gate, and the obligation to come back to it

The executable lanes did not say what [`../reference/testing-lanes.md`](../reference/testing-lanes.md) grades. That page calls acceptance informational, while `profile.pre-push` selected `kind(test)`, which pulled the acceptance binary in with the rest of the integration lane. On a host without virtualization the trials gate themselves and the difference never shows. On a capable host the stage failed, and the cost was never the four reds — it was the twenty trials passing beside them going unread, because a stage that always fails is a stage that gets pushed past.

Owed, in this order, and each step is a revision to this section:

1. Taken 2026-08-12: `profile.pre-push` now selects `kind(test) - binary(user_workflows)`, so the executable form states the grading the lane table already carried, and acceptance gates the host runbooks and CI instead. The alternative — an `#[ignore]` per trial naming its slice — was refused as four knobs whose removal nothing enforces. This subtraction is the knob, it is one line, and steps 2 and 3 are what remove it.
2. Taken 2026-08-12 by slice 013: the whole-binary exclusion is gone, and `profile.pre-push` now subtracts three named trials — `workflow_05_restrict_egress_allowlist`, `workflow_07_stop_restart_preserving_volumes`, and `workflow_08_destroy_cold_rebuild`. Measured on a capable host, the profile runs 23 tests and all 23 pass. Slice 014 removes two of the three by greening its own trials; slice 004 deletes the last clause and returns `profile.pre-push` to `kind(test)`. A slice that lands its trials and leaves the subtraction as it found it has moved a row without restoring the gate.
3. Taken 2026-08-13 by slice 014: `profile.pre-push` now subtracts exactly `- test(=workflow_05_restrict_egress_allowlist)`, and the table above is the replacement measurement rather than an amendment of the previous one. Three reds became one, which is the signal that the remainder belongs to slice 004 and to nothing in flight — and it is the check that the narrowing happened, since a subtraction nobody narrowed reports the same green either way. Measured after: `profile.pre-push` runs 26 tests and all 26 pass, twice, at 420s and 442s. Slice 004 deletes the last clause and returns the filter to plain `kind(test)`; that is the only step this section still owes.
4. Taken 2026-08-14 by slice 004: the last clause is gone and `profile.pre-push` selects plain `kind(test)`. Measured after: the profile runs 31 tests and all 31 pass, twice, at 399s and 364s — the count grew by the returned trial and by the four `tests/net_host.rs` trials the slice added. This section is owed nothing further; a future subtraction re-opens the obligation and names the slice that removes it.

## Known cost

This page restates cluster membership that each slice's `Governed by` section also carries, so the two can drift. The mitigation is scope, not process: this page names clusters and consuming slices and never restates a decision, and it is revised whenever a slice opens or closes. If a cluster here disagrees with a slice's `Governed by`, the slice wins and this page is wrong.
