# 017 — Doctor catalog

## Goal

The whole `spec/13` probe catalog runs as one shared registry, `viv doctor` reports it, and `viv start`'s preflight consumes the hard subset instead of its own one-off check.

## Appetite

4 implementation sessions.

## Core

One registry serves `doctor` and preflight so the two cannot drift, every probe in [`spec/13`](../../../reference/spec/13-doctor-and-health-checks.md) exists with its id as its diagnostic id, and `viv doctor` renders the fixed human report and `--json` envelope with the fixed exit mapping.

## In scope

Ordered; when the appetite binds, cut from the bottom.

1. Build the registry: a `Probe` descriptor carrying `id`, `category`, `scope`, `severity`, and `title`, plus a run function over injected host facts, so `--list` enumerates without probing, preflight filters by severity, and the catalog is testable without the host conditions it describes. The one existing calculation, [`src/doctor/descriptors.rs`](../../../../src/doctor/descriptors.rs), becomes the `host-fd-limit-sufficient` probe.
2. Implement the hard host probes — `nix-present`, `nix-version`, `nix-flakes-enabled`, `kvm-device-present`, `kvm-device-accessible`, `hardware-virt-available`, `host-userns-available`, `runtime-dir-usable` — and replace the `/dev/kvm`-only `preflight()` in [`src/cli/lifecycle.rs`](../../../../src/cli/lifecycle.rs) with a run of this subset, its `host.no-kvm` diagnostic becoming `kvm-device-present`.
3. Wire the verb: `viv doctor [--json] [--strict] [--list] [--online]` through the grammar, the category-grouped human report with bracketed word markers and the summary line, the `--json` envelope with `summary.hard_failures` and `schema_version`, and the exit mapping — first hard failure in catalog order decides the code, `--strict` on a warn exits `1`.
4. Implement the soft host probes: `nix-store-disk-space`, `state-dir-free-space`, `host-memory-headroom`, `host-cgroup2-delegation`, `kernel-version-supported`, `host-landlock-available`, `state-dir-writable`, `state-files-parse`, `cache-dir-writable`, `data-dir-writable`, `store-roots-intact`, `host-linger`.
5. Implement the soft project probes — `config-parses`, `manifest-resolves`, `shared-layer-paths-portable`, `manifest-no-inline-secret`, `mount-source-not-session-dir`, `lock-covers-declared-inputs`, `agent-source-usable` — skipped with reason `no-manifest-bound` and the one stderr note when nothing is bound.
6. Implement the network probes behind `--online`: `nix-version-currency`, `substituter-reachability`, `egress-allowlist-dns`, skipped with reason `offline-mode` by default.
7. Update [`implementation-status.md`](../../../reference/implementation-status.md) and the owning explanation page, and verify the catalog on a real host with the findings recorded here.

## Out of scope

- Any repair or mutation; `doctor` diagnoses and changes nothing.
- New probes beyond the `spec/13` catalog, and any change to that catalog's membership.
- The documentation site behind `doc_url`; the path scheme ships, the host does not.
- Guest-side health checks and the evaluation-time assertions `spec/13` excludes by construction.

## Governed by

- [`../../../decisions/ADR-0023-doctor-check-catalog-and-contract.md`](../../../decisions/ADR-0023-doctor-check-catalog-and-contract.md) — fixes the catalog contract, `--strict`'s exit `1`, and the one-catalog rule.
- [`../../../reference/spec/13-doctor-and-health-checks.md`](../../../reference/spec/13-doctor-and-health-checks.md) — owns every probe, the report shapes, and the exit mapping.
- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — owns the codes the hard probes name and the shared id space.
- [`../../../reference/spec/01-command-surface.md`](../../../reference/spec/01-command-surface.md) — owns the verb's place on the surface and the preflight rule.
- [`../../../decisions/ADR-0015-cli-output-and-failure-contract.md`](../../../decisions/ADR-0015-cli-output-and-failure-contract.md) — owns streams and `--json` for the report.
- [`../../../decisions/ADR-0069-redaction-is-by-construction.md`](../../../decisions/ADR-0069-redaction-is-by-construction.md) — binds the probes that look near credentials.

## Acceptance

When `viv doctor` runs, every catalog probe SHALL report `pass`, `warn`, `fail`, or `skipped` with a reason, grouped by category with bracketed word markers and a closing summary line naming the exit.

When a hard probe fails, the exit code SHALL be the first failing probe's code in catalog order, and `viv start` SHALL refuse on the same probe with the same diagnostic id before any side effect.

When `viv doctor --json` runs, stdout SHALL carry exactly one enveloped record with `summary.hard_failures` and `schema_version`, and stderr SHALL carry no part of the result.

If `--strict` is given and at least one soft probe warns with no hard failure, the exit SHALL be `1`, and this SHALL remain the only non-sysexits code on the surface.

When no manifest is bound, project probes SHALL skip with reason `no-manifest-bound`, one stderr note SHALL say so with `viv init` guidance, and all host probes passing SHALL still exit `0`.

While `--online` is absent, network probes SHALL skip with reason `offline-mode` and no probe SHALL touch the network.

## Rabbit holes

- Probes drift into repairs or evaluation — escape: `doctor` reads; the authoritative refusals stay at evaluation and launch where `spec/13` places them.
- A second probe list grows beside the registry — escape: one registry, three call sites; `--list` and preflight filter it.
- The textual lints grow expansion or resolution — escape: `spec/13` fixes them as best-effort text checks; the real gates already exist.
- Host facts get probed in the agent environment and promoted — escape: the harness method note owns this; host-tier claims are verified on a real host or marked unverified.
- The JSON envelope grows fields the spec does not name — escape: additive fields are ADR-0075 territory, not this slice's.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, the real-host run is recorded here, and the [`milestones.md`](../../milestones.md) row flips to `done` with slice 005 next.

## Revisions

Recorded 2026-08-17, at close. The seven items landed in one merged pass under the appetite, nothing cut. The acceptance evidence, in order: a full `viv doctor` on this host reported all 31 probes grouped by category with bracketed markers and the closing summary (`26 pass - 1 warn - 0 fail - 4 skipped -> exit 0` bound, `20 pass` unbound with the one stderr note); the cgroup-delegation warn is a true reading of this host (`pids` only delegated), and `--strict` promoted it to exit `1`; `--json 2>/dev/null` parsed as one object with `summary.hard_failures` and `schema_version`; `--online` produced a true currency finding (nix 2.34.8 trailing 2.35.2) and a reachable substituter; the preflight now runs the same hard subset — `workflow_01_first_time_bind_boot` booted a real VM through it — and `host.no-kvm` is gone, replaced by the probe ids; `workflow_16_doctor_report` holds the published shapes gate-safely. The pure judgments (version floors, the three lints, the name comparison, `MemAvailable`) carry unit tests; the host-condition halves are verified on this host only and stay unverified elsewhere, per the harness method note. `agent-source-usable` skips as `not-applicable` — no agent channel is declarable in the manifest grammar yet — which is the honest answer rather than a vacuous pass.
