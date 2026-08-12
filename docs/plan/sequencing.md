# Sequencing

A decision record says whether a choice was made. It never says when that choice gets built, and its [`## Status`](../decisions/template.md) vocabulary has no word for waiting. That is deliberate: status is frozen from `Proposed` onward, so sequencing cannot live in the decision. It lives here.

This page maps decision clusters onto the slices that consume them. It names clusters and their consuming slice only; what each decision decided stays in the decision, and [`../decisions/`](../decisions/) owns the inventory. Status stays in [`milestones.md`](./milestones.md).

Read it to answer one question: a decision exists, so when does it become code?

## Standing constraints

These bound every phase and no slice re-decides them, so they are not sequenced. `ADR-0001` fixes the isolation boundary, `ADR-0008` the two-layer separation, `ADR-0010` that secrets never enter the store, `ADR-0024` the backend security requirements, and `ADR-0025` the default hypervisor. A slice that appears to need one of these relaxed has found a charter question, not a sequencing one.

A decision consumed by a closed slice is already realized; [`../reference/implementation-status.md`](../reference/implementation-status.md) records how far. `ADR-0016`, `ADR-0065`, and `ADR-0071` appear below anyway, because their guest half is realized and their host half is not.

## Phase 1 — a sandbox that runs

The chain in flight. These clusters are consumed by slices 011 through 014, and each slice's `Governed by` section carries the individual links.

- Manifest, composition, and artifact model — `ADR-0002`, `ADR-0003`, `ADR-0004`, `ADR-0061`. Consumed by [slice 011](slices/011-resolve-and-evaluate/README.md).
- Config roots, binding, and precedence — `ADR-0005`, `ADR-0006`, `ADR-0011`, `ADR-0026`, `ADR-0029`, `ADR-0045`. Consumed by slice 011.
- State layout and durability — `ADR-0052`, `ADR-0053`, `ADR-0058`, `ADR-0059`. Consumed by slice 011.
- CLI output and failure contract — `ADR-0015`, `ADR-0028`, `ADR-0033`. Consumed by slice 011, with the `start` boundary settled in [slice 012](slices/012-first-boot/README.md) through Q-008.
- Lifecycle and runtime ownership — `ADR-0013`, `ADR-0018`, `ADR-0030`, `ADR-0055`, `ADR-0056`, `ADR-0097`. Consumed by slice 012 for the boot itself, and again by [slice 013](slices/013-exec-and-shell/README.md) for the reuse-or-boot routine a command needing a VM runs.
- Guest store provisioning — `ADR-0038`, `ADR-0084`, `ADR-0087`, `ADR-0088`. Consumed by slice 012.
- Open egress by default — `ADR-0007`. Consumed by slice 012 as the reason a first boot needs no network work.
- Guest control transport — `ADR-0016`, `ADR-0065`, `ADR-0071`. Guest half realized in slice 003; host half consumed by [slice 013](slices/013-exec-and-shell/README.md).
- Workspace, volumes, and disposability — `ADR-0009`, `ADR-0017`, `ADR-0019`, `ADR-0037`, `ADR-0043`, `ADR-0066`, `ADR-0067`, `ADR-0080`, `ADR-0092`. Consumed by slice 012 for the mount launch constructs and [slice 014](slices/014-workspace-and-persistence/README.md) for what persists; slice 014 item 2 settles which side `ADR-0066` and `ADR-0092` fall on.

## Phase 2 — funded and sequenced behind it

Shaped slices carrying the `later` status. They are not deferred indefinitely; they are sequenced behind a product that runs.

- Egress enforcement — `ADR-0007`, `ADR-0044`, `ADR-0064`. Consumed by [slice 004](slices/004-enforce-egress-allowlist/README.md).
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

## Known cost

This page restates cluster membership that each slice's `Governed by` section also carries, so the two can drift. The mitigation is scope, not process: this page names clusters and consuming slices and never restates a decision, and it is revised whenever a slice opens or closes. If a cluster here disagrees with a slice's `Governed by`, the slice wins and this page is wrong.
