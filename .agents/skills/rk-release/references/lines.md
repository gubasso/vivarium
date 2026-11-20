# Task: lines

Open a release line, patch it with a fix from the trunk, cut or promote a candidate on it, or retire it. A line serves a backport, which carries a fix back to a version users still run, and the `lines` release style, where a version passes through candidates before its final release. Follow `rk guide backport` and `rk guide release-lines` for the commands, and `rk method branch-for-release` for why a fix flows one way and the four ways the pattern breaks.

## Pre-flight

1. Run `rk lines list --target . --json`, and hold every line with its tags, its tag coverage, and its seat.
2. Hold `style` from `rk release status --target . --json`. Its `request` is the trunk's alone, so read each line's own release request through the forge, as `rk guide backport` step 7 shows.
3. Run `rk setup check --target .`, which asks the forge. Where it reports the `protect-release-lines` step unsatisfied, hand that one step to `rk-setup` first. Where it reports the step unknown, this runtime cannot observe the protection: say so, and continue. `rk setup status` reads only the committed proof, so it cannot see a rule removed since.

## Execution

By intent:

1. Open a line: preview `rk lines open <line> --base <tag> --target .`, then run it with `--apply` as a gated step. A line is cut from the tag it patches, so the base is never a default.
2. Patch a line:
   1. Confirm the fix is on the trunk first: its commit is reachable from the trunk. Getting it there is the project's own workflow.
   2. Cherry-pick only that commit onto the line, as a gated step. Never merge the line back into the trunk.
   3. Wait for the line's CI, then read the line's own release request.
   4. Release it through the `release` task, with the line as its trunk.
3. A candidate: on a rust, bash, or python pair, the operator mints it through the pair's dispatch or manual job, which runs `rk lines mint` and tags nothing by hand. `rk lines rc <line> --target .` reads the newest candidate and the next number. Validate the candidate, answer a finding by crossing its fix like any patch, promote the validated one with `promote` set, and release it through the `release` task.
4. Retire a line: preview `rk lines retire <line> --target .`, then run it with `--apply` as a gated step. It removes the seat before the branch, only behind the line's tags. Deleting the remote branch is the operator's step.

## Post-flight

1. Run `rk lines list --target . --json`, and confirm the line reads in its new state.
2. Fill the report's evidence line with what the run observed: `<line> opened at <tag>` where `rk lines open` created the line, `<line> adopted, base not checked` where it adopted a line that already existed, locally or at the remote, without using `--base`, or `<line> retired`. Where the run released a version through the `release` task, use that task's evidence line, prefixed with the line: `Release: <line> v<version> · published · artifacts verified`.
