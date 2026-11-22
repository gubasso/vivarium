# The landing pipeline

`land`, `migrate`, `adopt`, `upgrade`, `repair`, and `configure` fill these five steps. Each task file names its stage flags, its landing verb, and its own Prepare items. Run the task pre-flight before step 1 and the task post-flight after step 5. `rk guide landing` carries the commands by the same step numbers, and `rk method landing` carries the why.

## 1. Stage

1. Run `rk stage --target . --json` with the flags the task file names. On a recorded target, the configuration answers every choice the task does not change, so pass only the changed answers.
2. Hold the receipt's `stage_root`, written `<root>` below.

The stage writes nothing into the target. Keep it until the general post-flight cleans it, because a failure is diagnosed against it. Never copy the stage tree into the target.

## 2. Compare

1. Run `rk stage compare <root> --target . --json`.
2. Judge each row by its `state`, `kind`, `provenance`, and `word`. Where `history.available` is false, the history half of provenance is missing, and `history.reason` says why.

The word is the production decision, so it decides the row. Provenance, `attributed`, `reduced`, or `unknown`, explains a row and never changes what follows.

| Row                                         | What follows                                                                     |
| ------------------------------------------- | -------------------------------------------------------------------------------- |
| `equal`, or the word `created` or `matched` | nothing to decide                                                                |
| the word `replaced`                         | the landing refreshes it, and the plan names a `reduced` or `unknown` provenance |
| the word `preserved` or `drift`             | nothing to decide, because the landing keeps the project's bytes                 |
| the word `retired`                          | nothing to decide, because the landing removes a file nobody edited              |
| the word `collision`, `replaceable` true    | ask the operator, as a gated step, whether to keep the edit or discard it        |
| the word `collision`, `replaceable` false   | a refusal no flag passes: report it as the operator's fix by hand                |
| the word `dropped`                          | the task file says                                                               |

For a `collision` whose `replaceable` is true, both answers name the path with `--replace`, which saves a copy before it overwrites. Name no other row: `--replace` refuses every path the verb does not mark replaceable. Never land over a `collision` without an answer.

## 3. Prepare

Write only what the project owns, and write it before the landing:

- the answers the operator gave at the Compare step,
- the Prepare items the task file names.

## 4. Land

1. Run the task's landing verb with `--stage <root>` and without `--apply`, and show its output in the plan. Never list the destinations by hand.
2. After the operator approves, run the same verb with `--apply` and one `--replace <path>` per answered conflict, as a gated step.
3. Quote each word it prints. A refusal is a finding for the Compare step, never a flag to add.

## 5. Prove

1. Run `rk stage compare <root> --target . --json` again.
2. Confirm that every `rendered` and `region` row reads `equal`. A row with the word `retired` reads `equal` once the landing removed the file, and the state `retired` means the removal is still pending. A `seeded` or `state` row can differ, because the landing kept the project's bytes, and a `dropped` row can too, because the landing left it to the project.
3. Run `rk status --target . --json`, and confirm that `classification` reads `current`, or `drift` where only an unanswered sentinel remains.
4. Carry the result into the proof line of the report, and keep the stage.

Route each kept copy in the task post-flight: a kept workflow edit goes to `configure`, and anything else goes to "Still the operator's" in the report. A discarded copy stays where `--replace` saved it, and the report lists it.
