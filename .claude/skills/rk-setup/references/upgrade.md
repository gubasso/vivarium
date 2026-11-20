# Task: upgrade

Take a record older than the installed `rk` to this binary's projection, and apply any changed answers in the same landing. The gate picks it at `alignment` `binary-newer`. `rk method landing` carries the why.

## Pre-flight

1. Where the pin moved for this upgrade, the run follows this route. After the pin move, load the devshell and run the upgrade through the rk-setup skill of the rk in that devshell, and do every migration it names. Before `rk upgrade`, read it with `rk skill show rk-setup`. After it, the project's .claude/skills/rk-setup is that skill. Never use a host copy or an older skill. A home copy and the project copy that predates the landing both describe another release.
2. Read `config.pending` in `rk status --target . --json`. Ask each key the record predates, with its consequence. Ask first a key the upgrade refuses without, the release style: `trunk` ships from the trunk alone, and `lines` keeps older lines. In neither style does a release request merge itself.
3. A record that predates the integration mode reads as `forge`. State that, offer the change once, and add the answer to the Stage step's flags.
4. A record that predates the workflow guards reads them as a pending answer. Ask once whether to keep them or drop them, with keep as the default.
5. Read the guidance files the stage copies into `reference/guidance`, each release above the recorded one whose destinations the target has. `0.9.8.md` carries the order an upgrade to it takes: the registry answer, the upgrade, the gated dist jobs, the checkpoint, and one commit with the proof.
6. Read `legacy_state` in `rk status --target . --json`.
   - An entry with `active` true is an operation a 0.4.x release could still apply. Ask whether the operator finishes it with the old binary, or records that it is abandoned. The update waits for the answer. `rk guide landing` 1b carries the procedure.
   - Hold each inactive entry for the post-flight.

## Execution

Fill the pipeline:

- Stage: `rk stage --target . --json`, with the answers from the pre-flight as flags. The configuration answers the rest.
- Compare:

| Row                                 | What follows                                                                              |
| ----------------------------------- | ----------------------------------------------------------------------------------------- |
| `rendered`, `differs`, `attributed` | refreshed by the landing                                                                  |
| `differs`, `reduced` or `unknown`   | ask whether to keep the edit or discard it                                                |
| `retired`, the word `retired`       | removed by the landing, because nobody edited it since it landed                          |
| `dropped`, the word `dropped`       | ask whether to remove it, as a gated step after a commit, or keep it as the project's own |

- Land: `rk upgrade --stage <root> --target .`, then the same with `--apply` and one `--replace <path>` per answered conflict, as a gated step.

## Post-flight

1. The Prove step passed.
2. Regenerate what a landed configuration generates, such as the artifact plan, through the binding's step, before any check reads it. `rk guide setup` 4b carries the command.
3. Read `setup_proof` in `rk status --target . --json`. Where it reads `absent` or `stale`, the upgrade moved the setup contract: route to the `setup` task, then to `rk setup checkpoint --target .` on a host that holds the bot key, and commit the proof with the upgrade. Where a step this forge cannot run blocks the checkpoint, exclude it in `setup.excluded_steps` with its reason.
4. `rk status --target . --json` reports `classification` `current`.
5. Remove each dropped file the operator chose to remove, as a gated step.
6. Remove each inactive legacy path by its exact path, only under explicit cleanup authorization, as `rk guide landing` 5b carries it. No `rk` verb removes this state.
7. Route each saved copy as the pipeline says.
8. Where a formatter, a linter, or a spell checker excludes the landed skills, it excludes only the package folders `rk-setup/` and `rk-release/` under `.claude/skills/` and `.agents/skills/`. The project's own skills in those roots stay under its checks.
