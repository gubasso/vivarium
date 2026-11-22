# Task: configure

Change what a record selects, on a record at this binary's version: the Git workflow answers, the integration mode, the release style, and each opt-in capability. A changed answer is a landing at the same version with that answer as a flag. `rk upgrade` writes the answer back into `.release-kit/config.toml` with its comments kept, so never edit the configuration by hand. Two answers are the exception, because no flag carries them, so the operator writes them: the `[dev_environment]` table, and `profile.release.registry`.

## Pre-flight

1. Run `rk status --target . --json` for the current answers: the profile, the Git workflow, and the capabilities.
2. Run `rk profile --target . --json` for what the catalog can select here. Never offer a change the catalog reports unavailable.
3. State the consequence of each requested change in one line:
   - the checkout mode: branches open across the change follow the transition in `rk method worktrees`;
   - the integration mode: moving between `local` and `forge` rewrites the hook block and the routing block, and changes who integrates;
   - the release style: `trunk` ships from the trunk alone, and `lines` keeps older lines whose candidates a person validates;
   - a capability turned on lands its files, and turned off removes each one nobody edited and leaves an edited one as a dropped file whose removal is asked. The Scorecard workflow posts to a public API, and `codeql` reads the declared licence;
   - the workflow guards: on lands the agent and commit policy lines, the glossary, the workflow hooks, and two setup steps, and off retires them the same way;
   - the required check and the trunk name. `setup.required_workflow` is retired, and every verb refuses a configuration that still carries it.
4. Where the request is to change where a rust crate publishes, `profile.release.registry` between `crates-io` and `none`, state that a landing never rewrites the seeded `release-plz.toml`. The operator edits it by hand in the same change: `publish = false` with `git_only = true` for `none`, and `publish = true` with no `git_only` for `crates-io`. Then run `rk upgrade --apply`. `rk status --check` reports a `release-plz.toml` that disagrees with the answer.
5. Where the request is to run the release jobs in the project's own development environment, ask the operator for the answer: the command prefix, and the pinned steps or image that put the environment on the runner. Quote the matching example of `rk binding rust`, section "The development environment", as an example and never as a default. Read no manager file to propose a value. Before the landing, ask the operator to run `<run> release-plz --version` and to paste the output.

## Execution

Where the gate composes this task after a landing task, add its answers to that task's Stage flags and land once. Otherwise fill the pipeline:

- Stage: `rk stage --target . --json`, with only the changed answers as flags.
- Land: `rk upgrade --stage <root> --target .`, then the same with `--apply` as a gated step.

## Post-flight

1. The Prove step passed, and `rk status --target . --json` reports `current` with the new answers.
2. Regenerate what a changed configuration generates, as a gated step.
3. Name the follow-ups the change causes under "Still the operator's": an open release request goes to the `rk-release` skill, and branches open across a checkout-mode change follow `rk guide worktree`.
4. Remove each dropped file the operator chose to remove, as a gated step after a commit.
