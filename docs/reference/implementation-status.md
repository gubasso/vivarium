# Implementation status

The single source of truth for what vivarium does today versus what is designed only. The specification under [`spec/`](spec/README.md) describes the intended design; this page records how much of it exists in code.

## Current state: a project can be bound and inspected; nothing boots from a manifest yet

The Nix-built diagnostic runner now hands a strict launch contract to the Rust [policy constructor](../../src/launch/policy.rs), [transient-service renderer](../../src/launch/systemd.rs), and [supervisor](../../src/launch/supervisor.rs). The supervisor implements API create-before-boot ordering, one daemon per share, console ownership, group cancellation, allowlisted runtime cleanup, a current-boot agent ping, and initial credential-pool readiness. The shared [protocol](../../src/protocol/mod.rs) and private [guest agent](../../crates/vivarium-guest-agent/src/main.rs) implement bounded framing, process and PTY sessions, guest AF_VSOCK listeners, and declared SSH/GPG relays.

The first public commands now run. [`../../src/config/registry.rs`](../../src/config/registry.rs) persists the project→manifest binding in the state root under an exclusive sidecar lock, [`../../src/config/resolve.rs`](../../src/config/resolve.rs) resolves the effective manifest through the fixed precedence, and [`../../src/cli/`](../../src/cli/) turns both into `viv init`, `viv config`, and the `manifest` readers. The `Cli` gate that skipped every trial is open, and its six trials pass.

What is still gated is everything behind evaluation. No manifest is evaluated to a guest system derivation, so `viv config eval`, `viv config sources`, and every booting verb remain written-but-skipped. Their grammar and fail-closed paths do run — a malformed invocation answers `64` and an unbound project answers `78`, asserted by the usage trials named below — but the verbs themselves do nothing, and their rows stay at the middle level for that reason.

What is no longer merely written is the guest half. `tests/guest_agent_host.rs` passes on a capable host, twice, driven by [`../../tests/host/guest-agent-check`](../../tests/host/guest-agent-check): real guest boot, AF_VSOCK transport on both ports, guest PTY job control and pre-`Start` sizing, boot-identity refusal that executes nothing, socket ownership and modes under `/run/vivarium`, opaque credential bytes, and pool refill under more concurrent clients than the pool has slots. The refill needed a backend at or above cloud-hypervisor v53.0 and no vivarium code ([KI-0002](./known-issues/resolved/KI-0002.md)). Figures and host in the [verification harness](./microvm-verification-harness.md).

## Surface status

Each command sits at one of three levels:

- Designed — specified, with nothing written against it yet.
- Acceptance test written (gated) — a trial in [`../../tests/user_workflows.rs`](../../tests/user_workflows.rs) encodes the intended behavior and is linked below, and the behavior the row names does not run. Some of these commands do have a passing usage trial: their grammar and their fail-closed answer are real, and only the work behind them is missing. That is not enough for the top level, which is about the command doing its job, so the row stays here and its Trial column says which half is covered. A written trial is a specification made executable, not evidence that anything works.
- Implemented — the command runs and its trial passes.

Three rows are at `Implemented`. Every other public command is written-but-skipped or designed only.

| Command                               | Status                          | Trial                                                                                           |
| ------------------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------- |
| `viv init`                            | Implemented                     | `workflow_01_first_time_bind_usage`, `workflow_02_clean_repo_global_registry_only`              |
| `viv unbind`                          | Designed                        | —                                                                                               |
| `viv images list`                     | Designed                        | —                                                                                               |
| `viv manifest list` / `manifest show` | Implemented                     | `workflow_04_inspect_before_run_usage` (library reading only; the merged view is `config eval`) |
| `viv start`                           | Acceptance test written (gated) | `workflow_01_first_time_bind_boot`, `workflow_08_destroy_cold_rebuild`                          |
| `viv exec`                            | Acceptance test written (gated) | `workflow_06_exec_usage_surface`, `workflow_06_exec_exit_code_propagation`                      |
| `viv shell`                           | Acceptance test written (gated) | `workflow_01_first_time_bind_usage` (usage errors only; no interactive PTY coverage)            |
| `viv stop`                            | Acceptance test written (gated) | `workflow_07_volume_list_requires_binding`, `workflow_07_stop_restart_preserving_volumes`       |
| `viv destroy`                         | Acceptance test written (gated) | `workflow_08_destroy_usage_surface`, `workflow_08_destroy_cold_rebuild`                         |
| `viv update`                          | Designed                        | —                                                                                               |
| `viv generations`                     | Designed                        | —                                                                                               |
| `viv gc`                              | Acceptance test written (gated) | `workflow_08_destroy_usage_surface` (reachability only; no store-sweep coverage)                |
| `viv volume`                          | Acceptance test written (gated) | `workflow_07_volume_list_requires_binding`, `workflow_07_stop_restart_preserving_volumes`       |
| `viv status`                          | Acceptance test written (gated) | `workflow_01_first_time_bind_boot`, `workflow_07_stop_restart_preserving_volumes`               |
| `viv trim`                            | Designed                        | —                                                                                               |
| `viv config`                          | Implemented                     | `workflow_01_first_time_bind_usage`, `workflow_02_clean_repo_global_registry_only`              |
| `viv config sources`                  | Acceptance test written (gated) | `workflow_03_team_shared_and_personal_override`, `workflow_05_restrict_egress_config_surface`   |
| `viv config eval`                     | Acceptance test written (gated) | `workflow_03_team_shared_and_personal_override`, `workflow_04_inspect_before_run`               |
| `viv doctor`                          | Designed                        | —                                                                                               |

## Stability

This page says what works. This section says what a release may change, which is the reader's other question and belongs beside the first.

Before 1.0, the minor component is the breaking axis. A patch release — `0.x.y` to `0.x.y+1` — must not break a documented invocation, a `--json` field, an exit code's meaning, or accepted manifest and registry syntax. Breaking changes land only in a new minor, each named in the release notes with a migration example. Where practical a surface is deprecated for one minor first; an immediate break is reserved for security, correctness, and any surface that never reached Implemented above. Decided in [`../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md`](../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md).

Three surfaces are already permanent, now, pre-1.0 — earlier decisions made them so, and this promise does not weaken them:

- the exit-code taxonomy — never reassigned, append-only ([`spec/14-exit-codes.md`](spec/14-exit-codes.md));
- the two compatibility messages and all five parts each must carry — file, failing position, unknown key, accepted key set, CLI version. Neither the manifest nor the registry carries a schema version, so these messages are the compatibility surface rather than a courtesy ([`spec/14-exit-codes.md`](spec/14-exit-codes.md));
- the registry record's two keys, which `viv init` prints for a human to paste ([`spec/02-config-and-xdg-layout.md`](spec/02-config-and-xdg-layout.md)).

Diagnostic ids are stable and never reassigned, and are still not a branch surface — a consumer that must branch branches on the exit code. Both halves hold together; neither implies the other.

Outside the promise: human stderr prose beyond those normative messages and their rendering slots, additive `--json` fields, and the generated flake's internal shape, which is a regenerable cache artifact.

There is no experimental-feature gate before 1.0. At `0.x` the whole surface is unstable, so a per-feature gate would add a second stability vocabulary saying what the version number already says; the three levels above already stop a written-but-skipped trial from reading as a promise. A gate is the post-1.0 answer, if it is ever the answer.

## How to update this page

A command moves from Designed to Acceptance test written (gated) when a trial encodes its behavior — name the trial, and say plainly if the coverage is partial. It moves to Implemented only when the command runs and that trial passes unskipped; link the code that enacts it then. Any change that adds, removes, or renames a command, or changes its acceptance test, updates that command's row in the same change.

Keep this page honest: a reader deciding whether to rely on a feature consults it first, and a stale "Implemented" here is worse than none. Because the stability promise above leans on this table, honesty here is a checked obligation rather than a good intention. Three checks enforce it, and they belong in CI rather than in review:

1. every trial named in the Trial column resolves to a test that exists — this catches a rename;
2. no row claiming Implemented names a trial that the runtime gate skips or that is ignored — this catches the exact lie the paragraph above calls worse than none;
3. every subcommand the argument parser defines has a row here — this catches a command that shipped without a status.

The middle level exists precisely so a written-but-skipped trial cannot be mistaken for working software. This page owns status; the spec owns the contract. When a trial and the spec disagree about that contract, [`../../AGENTS.md`](../../AGENTS.md) carries the resolution rule — and a fact a trial is the only record of belongs in the spec page that owns it, not in the test file. Which lane a given trial belongs to, and what that lane proves, is in [`testing-lanes.md`](./testing-lanes.md).
