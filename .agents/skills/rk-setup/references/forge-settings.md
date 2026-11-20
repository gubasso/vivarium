# Task: forge-settings

Configure the settings of the forge that hosts the repository, GitHub or GitLab, which no file in the repository carries and `rk init` cannot land. It is `rk guide setup` §0 to §3, the steps `rk setup --list` names. On a first landing it runs before `land`, and elsewhere after the landing task.

## Pre-flight

1. Run `rk setup status --target . --json`. It reads offline, with no credential: the setup proof and each step's state.
2. Run `rk setup --target . --json` for the preview of every step.
3. Read a gap per step, in four words:
   - a current failure, from a check that could observe it: a step to run;
   - `unknown`, from a credential this runtime lacks: no step, and no question;
   - a compatible last proof: it still holds;
   - no proof, or a stale or invalid one: check again where the credentials exist.
4. List in the plan only the steps that are not `satisfied`, in the runbook's order.

## Execution

Per step, in order from §0 to §3:

1. Preview `rk setup step <name> --target . --json`.
2. Run `rk setup step <name> --target . --apply` as a gated step.
3. Print `install-bot` and `bot-secrets` for the operator with the walkthrough of `rk forge <name>`, because the credentials stay the operator's. `install-bot` reads `unknown` on any host without the App key.
4. Give a destructive step its own approval. `single-trunk` deletes branches, and a refusal from its ancestry guard is a stop, never an obstacle.
5. Read the path each step names in its `snapshot` event: a step that removes or replaces forge state saves what it found first. A step that cannot read that state refuses, and that is a finding.
6. Where `package-check` reports its limitation on a bash target, tell the operator to run `rk release build --list` before the first release, because no setup step lists the package. The list holds the tracked files that `profile.release.build.include` and `profile.release.build.exclude` select.

Never run the steps out of order, because each one names what the next depends on. Never propose a second request-reporting workflow as a gated check: the forge resolves a job's `needs` inside one file, which is the required-check decision of §3a and §3c.

## Post-flight

1. Run `rk setup check --target . --json` where the credentials exist. Elsewhere, report each step `unknown`, from observation.
2. On a target proven before, refresh the proof with `rk setup checkpoint --target .`, a gated step that commits `.release-kit/setup-proof.json`. On a first setup, `publish` records the proof, once the automated path is proven.
3. List every snapshot path under "Saved before changing" in the report.
