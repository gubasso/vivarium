---
name: rk-release
description: Runs the release-kit release process in a project where release-kit already landed, covering cutting a release, holding it or correcting its changelog, release lines with their backports and candidates, and recovery from a release that failed or stalled. Use when asked to cut or ship a release, hold a release, fix a changelog entry before release, backport a fix to an older line, cut or promote a release candidate, retire a release line, recover a release that published nothing or lost its artifacts, withdraw a version, or hand-publish while CI is down. Triggers include release, cut a release, publish a version, release request, backport, release line, release candidate, yank, and release recovery.
license: CC-BY-4.0
compatibility: Requires the rk binary on PATH, which the rk-setup skill provisions. Operating a release needs the forge CLI authenticated, and push access where a changelog correction or a merge is the operator's request.
---

# rk-release

Run release-kit's own release process in a project where release-kit already landed: a release, its release lines, and its recovery. The release state comes from `rk release status`, the request picks the task, and the operator confirms each task before it runs.

## Pre-flight

1. Before any other action, read `references/pre-flight-gate.md` and run it whatever the request carries. No flag skips it.
2. Hand off or stop on any row below, and write nothing. The evidence comes from `rk status --target . --json` unless the row names another source.

| Finding                                   | Evidence                          | What follows                                                |
| ----------------------------------------- | --------------------------------- | ----------------------------------------------------------- |
| No `rk` on PATH                           | `command -v rk`, by hand          | hand off to the `rk-setup` skill, which provisions it       |
| A record newer than the binary            | `classification` null             | hand off to `rk-setup`, which provisions the pinned version |
| No record, or a setup that is not current | `classification` is not `current` | hand off to `rk-setup`                                      |
| An unreadable record                      | a diagnostic instead of a report  | stop, and repair the file the diagnostic names              |

1. Run `rk release status --target . --json`, and hand its report to the gate. The gate reads nothing else.

## General rules

1. Run a release action only where the request names it: merging the release request, re-running a publish or artifact job, or withdrawing a version. Otherwise print the exact command for the operator.
2. Route, never restate. The sequence is `rk method operate`, the commands are `rk guide release`, `rk guide backport`, and `rk guide release-lines`, and the way back is `rk method recovery`. Never copy one into a plan.
3. Never author a tag. Automation tags every version, a candidate included, and a published version is never re-authored.
4. Never push the trunk. Work reaches it through the project's own workflow.
5. Never arm a release request. A release happens only on the operator's order, and an armed request is a defect `rk release status` reports as `request-armed`.
6. A trunk release is four steps: read the open release request and its changelog section; push a changelog correction, where one is needed, as the last commit on the request's branch; run `rk release merge --apply`; read the release state, and make sure that the tag, the release page, and the assets exist. Never merge a trunk release request through the forge by hand: the merge job proves the order as the release App. A release line's request keeps the merge by a person.

## Gate

Read the intent from the request and the state from `rk release status --target . --json`, never from the forge by hand. A field the runtime's credentials cannot read reads `unknown`: report it, and never ask for a secret to fill it.

| Intent                                            | Tasks     |
| ------------------------------------------------- | --------- |
| release, ship, hold, or correct the changelog     | `release` |
| a release line: open, patch, candidate, or retire | `lines`   |
| something broke or stalled                        | `recover` |
| none named, and `symptoms` is not empty           | `recover` |
| none named, and nothing pending                   | done      |
| start work, integrate a branch, or prune          | none      |

- Done: report the state from observation.
- None: answer in one line that this is the project's own workflow, and that `rk issue start`, `rk integrate`, and `rk worktree prune` exist as its helpers. Run none of them as a task.

The operator confirms every step:

1. Present "State: <request and last release>. Request: <intent>. Tasks: <list>. Proceed?" and wait. A redirect reads this table again with the new intent.
2. Before each task, print "Next: <task>. It will <one line>. Proceed?" and wait. Print one progress line at each move between the pre-flight, the execution, and the post-flight of a task.
3. After the task pre-flight, read `references/plan-gate.md` and hold it for the rest of the run. The plan names each task, the commands it runs, and the release actions it prints or runs.

## Tasks

After the gate picks a task, read its file into this context. A task file holds the pre-flight, the execution, and the post-flight of one task, and it links no other task file.

- If the run includes `release`, read `references/release.md`.
- If the run includes `lines`, read `references/lines.md`.
- If the run includes `recover`, read `references/recover.md`.

Compose them this way:

- Releasing a line runs `release` with the line as its trunk and the line's own release request.
- A release that must not ship is the hold of `release`, and a fix an older line needs is `lines`. `recover` takes only the paths no other task covers.
- Where `release/*` is not protected, `lines` hands the `protect-release-lines` step to `rk-setup` before it opens or patches a line.

## Post-flight

Run these once, after the last task post-flight, in this order.

1. Check. Run `rk status --check --target .`.
2. Report. Fill the template from observation, with every answer the operator gave.

On a failed check, report and route: a violation on a file release-kit owns goes to `rk-setup`. Never repair inside the post-flight.

```text
Result: <the exact rk status --check line, or the first failing line>
Tasks:
  - <task>: <files written or removed>; proof: <task post-flight evidence>
Answers recorded:
  - <question>: <answer>
Git and forge actions:
  - <action>: <run under the request | printed for the operator>
Release: <v<version> · published · artifacts verified | held at <step> | failed: <symptom> | <line> opened at <tag> | <line> adopted, base not checked | <line> retired>
Saved before changing:
  - <path>: <what it holds>
Still the operator's:
  - <exact command>: <what it changes>
Next: <one action>
```

Print "Git and forge actions" and "Saved before changing" only where the run has an entry for them.
