# Implementation status

The single source of truth for what vivarium **does today** versus what is **designed only**. The specification under [`spec/`](spec/README.md) describes the intended design; this page records how much of it exists in code.

## Current state: design stage

**No command is implemented yet.** The repository contains the founding documentation, a binary skeleton that does nothing, and an acceptance harness whose trials are written but cannot run. No command in [`spec/01-command-surface.md`](spec/01-command-surface.md) is functional. Do not treat any spec page as a description of working behavior; treat it as the target.

## Surface status

Each command sits at one of three levels:

- **Designed** — specified, with nothing written against it yet.
- **Acceptance test written (gated)** — a trial in [`../../tests/user_workflows.rs`](../../tests/user_workflows.rs) encodes the intended behavior and is linked below. It does not pass; it is **skipped**, because its runtime gate requires a working `viv` (and, for some, Nix and `/dev/kvm`). A written trial is a specification made executable, not evidence that anything works.
- **Implemented** — the command runs and its trial passes.

Nothing is at **Implemented** today.

| Command                               | Status                          | Trial                                                                                         |
| ------------------------------------- | ------------------------------- | --------------------------------------------------------------------------------------------- |
| `viv init`                            | Acceptance test written (gated) | `workflow_01_first_time_bind_usage`, `workflow_02_clean_repo_global_registry_only`            |
| `viv unbind`                          | Designed                        | —                                                                                             |
| `viv images list`                     | Designed                        | —                                                                                             |
| `viv manifest list` / `manifest show` | Acceptance test written (gated) | `workflow_03_team_shared_and_personal_override`, `workflow_04_inspect_before_run_usage`       |
| `viv start`                           | Acceptance test written (gated) | `workflow_01_first_time_bind_boot`, `workflow_08_destroy_cold_rebuild`                        |
| `viv exec`                            | Acceptance test written (gated) | `workflow_06_exec_usage_surface`, `workflow_06_exec_exit_code_propagation`                    |
| `viv shell`                           | Acceptance test written (gated) | `workflow_01_first_time_bind_usage` (usage errors only; no interactive PTY coverage)          |
| `viv stop`                            | Acceptance test written (gated) | `workflow_07_volume_list_requires_binding`, `workflow_07_stop_restart_preserving_volumes`     |
| `viv destroy`                         | Acceptance test written (gated) | `workflow_08_destroy_usage_surface`, `workflow_08_destroy_cold_rebuild`                       |
| `viv generations`                     | Designed                        | —                                                                                             |
| `viv gc`                              | Acceptance test written (gated) | `workflow_08_destroy_usage_surface` (reachability only; no store-sweep coverage)              |
| `viv volume`                          | Acceptance test written (gated) | `workflow_07_volume_list_requires_binding`, `workflow_07_stop_restart_preserving_volumes`     |
| `viv status`                          | Acceptance test written (gated) | `workflow_01_first_time_bind_boot`, `workflow_07_stop_restart_preserving_volumes`             |
| `viv trim`                            | Designed                        | —                                                                                             |
| `viv config`                          | Acceptance test written (gated) | `workflow_01_first_time_bind_usage`, `workflow_02_clean_repo_global_registry_only`            |
| `viv config sources`                  | Acceptance test written (gated) | `workflow_03_team_shared_and_personal_override`, `workflow_05_restrict_egress_config_surface` |
| `viv config eval`                     | Acceptance test written (gated) | `workflow_03_team_shared_and_personal_override`, `workflow_04_inspect_before_run`             |
| `viv doctor`                          | Designed                        | —                                                                                             |

## How to update this page

A command moves from **Designed** to **Acceptance test written (gated)** when a trial encodes its behavior — name the trial, and say plainly if the coverage is partial. It moves to **Implemented** only when the command runs and that trial passes unskipped; link the code that enacts it then.

Keep this page honest: a reader deciding whether to rely on a feature consults it first, and a stale "Implemented" here is worse than none. The middle level exists precisely so a written-but-skipped trial cannot be mistaken for working software. This page owns status; the spec owns the contract. When a trial and the spec disagree about that contract, [`../../AGENTS.md`](../../AGENTS.md) carries the resolution rule — and a fact a trial is the only record of belongs in the spec page that owns it, not in the test file.
