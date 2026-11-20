# Task: adopt

Record a target whose release-kit landing is present and whose record is missing, typically one an older release landed before the record existed. Its files differ from the candidate for that reason alone. `rk method migration` carries the why.

## Pre-flight

1. Run `rk profile --target . --json`. It reports what detection resolves.
2. Ask only what it leaves open, with the questions and the defaults of the `land` task. An adoption asks for the release style explicitly, and lands the workflow guards only where the target already carries them.

## Execution

Fill the pipeline:

- Stage: `rk stage --target . --json`, with the answers as flags.
- Compare: no receipt exists, so provenance comes from Git alone. Group the rows by their history:

| Row                                                            | What follows                                                      |
| -------------------------------------------------------------- | ----------------------------------------------------------------- |
| any word but `collision`                                       | nothing to decide                                                 |
| `collision`, `replaceable` true, `edited_after_added` not true | one answer for the whole group: replace them all, each copy saved |
| `collision`, `replaceable` true, `edited_after_added` true     | ask per file whether to keep the edit or discard it               |
| `collision`, `replaceable` false                               | a refusal no flag passes: report it as the operator's fix by hand |

- Land: `rk adopt --stage <root> --target .`, then the same with `--apply` and one `--replace <path>` per answered file, as a gated step.

## Post-flight

1. The Prove step passed, and each sentinel is answered.
2. `rk status --target . --json` reports `classification` `current`. The record now stands at this binary's version, so no `upgrade` follows.
3. Route each saved copy as the pipeline says.
