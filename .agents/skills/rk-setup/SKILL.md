---
name: rk-setup
description: Lands the release-kit release workflow in a project through the rk CLI, migrates, adopts, upgrades, repairs, and configures it, sets up the forge and the registry it releases through, and provisions the rk binary it runs on. Use when asked to set up a release workflow, release automation, trusted publishing, release-plz, git-cliff, changelog automation, or trunk-based release automation, to migrate a project that already releases through another tool onto release-kit, to take a landed project to the installed release, to repair setup drift, to change a recorded answer or an opt-in capability, or to install or pin rk for a project. Triggers include release-kit, rk init, rk stage, rk upgrade, rk adopt, rk setup, release setup, migrate to release-kit, adopt release-kit, upgrade release-kit, setup drift, vulnerability reporting, SECURITY.md, and pin rk.
license: CC-BY-4.0
compatibility: Provisions the rk binary where the host lacks it. Landing files into a target needs write access to that repository. Forge mutations need the forge CLI authenticated with administration rights; the operator supplies credentials and registry actions.
---

# rk-setup

Set up the release-kit convention in a project. Keep a landed project current with the installed `rk`. Provision the `rk` it runs on. Where a file named here is missing, `rk skill show rk-setup --file <path>` prints it. Each run takes three inputs:

- The state of the target decides which tasks are possible.
- The request picks among them.
- The operator confirms each task before it runs.

## Pre-flight

1. Before any other action, read `references/pre-flight-gate.md` and run it whatever the request carries. No flag skips it.
2. Stop on any row below. State the evidence and the operator's step. Write nothing. The evidence comes from `rk status --target . --json` unless the row names another source.

| Stop                                 | Owner       | Evidence                      | Operator's step                 |
| ------------------------------------ | ----------- | ----------------------------- | ------------------------------- |
| A failed `hard` probe                | shared gate | `rk doctor`                   | its `next` line                 |
| A failed `skill-shadow` probe        | shared gate | `rk doctor`                   | its `next` line                 |
| Not the repository the request means | shared gate | `git remote get-url origin`   | name the repository             |
| A diagnostic, not a report           | this skill  | the command exits on `reason` | repair the file it names        |
| Not a non-bare Git work tree         | this skill  | `git_work_tree` is false      | make the target a Git work tree |

An unreadable record never counts as an absent one, so never land over it.

1. Hold these observations for the gate. None of them ends the run. Read `alignment` before `classification`, because a record newer than the binary reads `classification` null.

| Observation                                            | Evidence                                  | Routes to                               |
| ------------------------------------------------------ | ----------------------------------------- | --------------------------------------- |
| No `rk` on PATH                                        | `command -v rk`, by hand                  | `provision`, then this pre-flight again |
| A record newer than the binary                         | `alignment` `target-newer`                | `provision`                             |
| A pin behind the latest release                        | `pin.behind` in the status report         | `provision`, then `upgrade`             |
| No pin and no host decision, at `setup` or `migration` | `rk self-depend status --target . --json` | `provision`, offered first              |
| A pin with no check line, at `setup` or `migration`    | `wired` and `envrc_check` in that report  | `provision`, offered first              |
| The state of the target                                | `classification`                          | the gate                                |
| The setup steps and the setup proof                    | `rk setup status --target . --json`       | `forge-settings` and `publish`          |

1. Hand the gate these inputs. The gate reads nothing else.
   - The status report.
   - Each failed `soft` probe, as a plan constraint.
   - The self-depend and setup reports.

## General rules

1. Route, never restate. Find the why with `rk method <chapter>` and the commands with `rk guide <runbook>`. Find a technology with `rk binding <tech>` and a forge with `rk forge <name>`. Find the whole command surface with `rk usage`.
2. Decide from the `--json` output of a verb, never from its human text. Never read "already current" from the words of an upgrade preview.
3. Leave rendered bytes to release-kit. Never edit a file or a region that `rk` renders. Never hand-edit a generated artifact workflow. Change its configuration, as the binding directs. Then regenerate it. Never author a tag.
4. Use an `rk` verb first. Where no verb covers a step, use the forge CLI. Say that the step runs outside the convention.
5. Report from observation. Report each step's outcome from what a check observed, never from the operator's word alone. A credential this runtime cannot reach leaves a step `unknown`, which is an observation boundary. Never ask for a secret so that a check can pass.
6. Keep one record per repository. Every landing carries the skills beside it. No step installs a skill under the home.
7. Let the repository record how it obtains `rk`. The `provision` task proposes the method, the operator picks it, and every install runs as a gated step.
8. Keep no state of your own, with one exception. These are the records:
   - The target.
   - Its record.
   - The stage receipt.
   - Git.

   A later run reads the state from `rk status --target . --json`. A migration's workspace is the exception. It keeps these across runs:
   - The archived files.
   - The snapshots.
   - The registry.

## Gate

Read the state from `classification`, never from the request, and the intent from the request. A cell is one ordered task list, and its tasks share one plan and one approval.

| State            | set up or none                      | upgrade   | fix       | change a setting            | forge setup                         | pin or environment |
| ---------------- | ----------------------------------- | --------- | --------- | --------------------------- | ----------------------------------- | ------------------ |
| `setup`          | `forge-settings`, `land`, `publish` | ask       | ask       | `land`                      | `forge-settings`, `land`, `publish` | `provision`        |
| `migration`      | `migrate`                           | ask       | ask       | `migrate`                   | `migrate`                           | `provision`        |
| `needs-decision` | ask                                 | ask       | ask       | ask                         | ask                                 | `provision`        |
| `adopt`          | `adopt`, then the setup tasks       | `adopt`   | `adopt`   | `adopt`, then `configure`   | `adopt`, then the setup tasks       | `provision`        |
| `upgrade`        | `upgrade`                           | `upgrade` | `upgrade` | `upgrade`, then `configure` | `upgrade`, then the setup tasks     | `provision`        |
| `drift`          | `repair`                            | `repair`  | `repair`  | `repair`, then `configure`  | `repair`, then the setup tasks      | `provision`        |
| `current`        | the setup tasks, else done          | done      | done      | `configure`                 | the setup tasks                     | `provision`        |

- Put `provision` first where an observation routes to it. With no `rk`, it is the only task. After it, run the pre-flight again. Then read this table in the same run.
- On a first landing, keep the order that `rk guide setup` takes. Its sections map to tasks:
  - §0 to §3: `forge-settings`.
  - §4: `land`.
  - §5 to §8: `publish`.

  On a recorded target, run the tasks in this order:
  - The landing task.
  - `configure`.
  - `forge-settings`.
  - `publish`.

  `configure` needs a record at the binary's version.
- The first setup task is `forge-settings`, where the setup report names a step that is not `satisfied`. The second is `publish`, where it reads the proof `absent` or `stale`. `migrate` runs both itself, once its branch merged.
- Ask: state the evidence and the options. Wait. Then read this table again with no new pre-flight. For `needs-decision`, the operator says what the tags or the second branch are.
- Hand off to the `rk-release` skill:
  - A release.
  - A release line.
  - A release recovery.
- Out of scope: starting work, integrating a branch, and pruning are the project's own workflow. Say so in one line. Name the helper verb that exists: `rk issue start`, `rk integrate`, `rk worktree`, or `rk branches prune`.
- Done: report from observation. For an upgrade asked at `current`, state "The record matches `rk <version>`. To move further, ask to move the pin.", which is a request that runs `provision`, then `upgrade`.

The operator confirms every step:

1. Present "State: <state>. Request: <intent>. Tasks: <list>. Proceed?" Wait for the answer. A redirect reads this table again with the new intent.
2. Before each task, print "Next: <task>. It will <one line>. Proceed?" Wait for the answer. Print one progress line each time a task moves to its next part.
3. After the task pre-flights of the cell, read `references/plan-gate.md` and hold it for the rest of the run. The plan names these:
   - Each task.
   - The verbs it runs.
   - The files it writes.

## Tasks

After the gate picks a task, read its file into this context. A task file links no other file. It holds three parts of one task:

- The pre-flight.
- The execution.
- The post-flight.

- If the run includes `provision`, read `references/provision.md`.
- If the run includes `land`, read `references/land.md`.
- If the run includes `migrate`, read `references/migrate.md`.
- If the run includes `adopt`, read `references/adopt.md`.
- If the run includes `upgrade`, read `references/upgrade.md`.
- If the run includes `repair`, read `references/repair.md`.
- If the run includes `configure`, read `references/configure.md`.
- If the run includes `forge-settings`, read `references/forge-settings.md`.
- If the run includes `publish`, read `references/publish.md`.
- If the run includes any of these, read `references/landing.md`, the pipeline they share:
  - `land`
  - `migrate`
  - `adopt`
  - `upgrade`
  - `repair`
  - `configure`

Compose them this way:

- These tasks run their task pre-flight first:
  - `land`
  - `adopt`
  - `upgrade`
  - `repair`
  - `configure`

  Then they run these steps of the pipeline, in order:
  - Stage.
  - Compare.
  - Prepare.
  - Land.
  - Prove.

  Last, they run their task post-flight. `migrate` runs the pipeline inside its own Land step.
- Where the gate composes `configure` after a landing task, its answers join that task's Stage step, so the run lands once. `configure` runs its own landing only at `current`.

## Post-flight

Run these once, after the last task post-flight, in this order.

1. Check. Run `rk status --check --target .`. Under local integration, confirm the `pre-merge-commit` hook, the gate every integration runs. On git 2.54 or newer, run `git hook list pre-merge-commit`. On older git, `git hook list` does not exist. There, report the listing as unavailable, not the hook as missing. Rely on the hook manager's install output instead. Where git lists no hook, state in the report that the clone has no gate yet. Name the install command of the project's hook manager.
2. Clean. Where this run staged and every check passed, print `rk stage clean <path>`. Wait for the answer. On a yes, run it. It removes the stage alone. These stay, and the report lists each path:
   - Saved copies.
   - Forge snapshots.
   - A migration workspace.
3. Report. Fill the template from observation, with every answer the operator gave.

On a failed check, keep the stage. Report the failure. Route it as follows. An `rk` violation goes to a new run, whose gate picks `repair`. A failing project check is the project's own. Never repair inside the post-flight.

```text
Result: <the exact rk status --check line, or the first failing line>
Tasks:
  - <task>: <files written or removed>; proof: <task post-flight evidence>
Answers recorded:
  - <question>: <answer>
Git and forge actions:
  - <action>: <run under the request | printed for the operator>
Setup proof: <compatible | stale | absent | unknown, because <reason>>
Saved before changing:
  - <path>: <what it holds>
Still the operator's:
  - <exact command>: <what it changes>
Stage: <cleaned | kept at <path>, because <reason>>
Next: <one action>
```

Print "Git and forge actions" and "Saved before changing" only where the run has an entry for them.
