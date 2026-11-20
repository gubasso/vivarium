# Task: repair

Bring a record at this binary's version back to clean, where `rk status --check` fails. The violations are rendered drift, a missing landed file, record drift, an unanswered sentinel, and an invariant failure. `rk method landing` carries the why.

## Pre-flight

1. Run `rk status --check --target . --json` and sort its `violations` by kind.
2. Report and never repair the informational lines: seeded drift is the project's, a stale pin moves with the next upgrade, and a warning is the operator's call.
3. A failing project check, such as a test or the project's own lint, is not drift. Report it as the project's, and invent no fix.

## Execution

Per violation kind:

- Rendered drift, a missing file, or record drift: fill the pipeline at the binary's own version.
  - Stage: `rk stage --target . --json`, with no flags, so the recorded values answer.
  - Compare: route each row by the pipeline's table. Ask about each `collision` whose `replaceable` is true whether to keep the edit or discard it. Both answers save a copy before the file is replaced. Report any other collision, such as a link where a landed file belongs, as the operator's fix by hand.
  - Land: `rk upgrade --stage <root> --target .`, then the same with `--apply` and one `--replace <path>` per answered collision, as a gated step.
  - A drifted rendered file has one more answer beside keeping or discarding the edit: an entry under `landing.accepted_drift` in `.release-kit/config.toml`, with the reason and the digest `rk status` prints, which keeps the edit and clears the violation. Where a parameter reaches the change, setting it and landing again needs no entry. The entry is the operator's to write.
- An unanswered sentinel: ask the project's value, and write it into the seeded file as a gated edit. `sentinel_locations` in `rk status --target . --json` names each one by file and line.
- An invariant failure: apply its `remediation` line from the report as a gated edit to the project's file. A remediation that changes a recorded answer goes to `configure`.

## Post-flight

1. The Prove step passed, where the pipeline ran.
2. `rk status --check --target .` passes, and `classification` reads `current`.
3. Route each saved copy as the pipeline says.
