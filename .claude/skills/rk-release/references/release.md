# Task: release

Ship the version the release request proposes, or hold it, or correct its changelog first. The gate picks it for a release intent on a landed, current project. Follow `rk guide release` for each step's commands, by its step numbers; this file says which steps a run takes and which stay the operator's.

## Pre-flight

1. Hold `style`, `symptoms`, and `request` from `rk release status --target . --json`: the request's number, `proposed_version`, `checks`, and `behind_trunk`. That report observes the trunk alone. For a line, read the line's own request as `rk guide backport` step 7 does.
2. Where `symptoms` is not empty, hand them to `recover` first.
3. Where the request asks to hold a version whose request already merged, it is a release that must not ship: hand it to `recover`.
4. Where `request.state` is `none` and no symptom shows, no release request is open: report that, and stop. It does not prove the trunk matches the last version, because the bot can still be opening the next request.
5. Where `behind_trunk` reads yes, the request waits for the bot's refresh and its checks, which is the drawn model of `rk method operate`, not a stall.

Work that lands while the request is open ships in its refresh or the next release, which is never a reason to rush a merge.

## Execution

First, as the run's first planned step, run `rk setup step package-check --target . --apply`, and confirm it exits 0: the trunk is releasable. Without `--apply` the step only names the commands it would run. It changes no tracked file, though the binding's packaging tool writes its build output, such as cargo's under `target/`. Where it fails, stop before any release action. How the work reached the trunk is the project's own workflow.

No request merges itself under either style, so the operator's one question is whether the changelog entry is complete and the release ships now.

1. To hold, do nothing: the request waits for the operator's order. A request that already merged is a withdrawal, not a hold, so hand it to `recover`.
2. To correct the changelog, compare the entry against the commit range (3b), correct it on the request's branch (3c), and confirm the request survived the bot's refresh (3d). The correction is the last step before the merge. Check the bot's branch out the way the project likes, with `rk worktree add` as one helper.
3. To merge on the trunk, preview with `rk release merge --target .` once the checks pass, and confirm it names the request and the version the operator read (4a). Then run `rk release merge --target . --apply` as the release action (4b). It dispatches the merge job and waits for that one run. Where it refuses, report the proof it names, and stop: the request stays open.
4. Bind the merge commit the release runs on (4c), from the verb's report, and hold its commit ID.

## Post-flight

Where the run held the request or corrected its changelog, and nothing merged:

1. After a hold on the trunk, run `rk release status --target . --json`, and confirm that the request is still open and that no `request-armed` symptom shows. On a line, a hold is a merge not made: read that request again as `rk guide backport` step 7 does, and confirm it is still open.
2. After a correction, read the changelog entry on the request's branch again, as the guide's step 3d does, and compare it with the correction the run made. Where a refresh overwrote it, redo the correction (3c). Where it cannot be read, report it as unknown.
3. Fill the report's evidence line as `held at <step>`.

After a merge on the trunk:

1. Run `rk release status --target . --wait <sha> --json`. It waits for the publish run and the artifact run on that commit, or on a bash or python pair for the one release run, selected by the commit and never by recency.
2. Where it exits 1, a run failed: hand the symptom it names to `recover`. Where it exits 73, the wait timed out: report the release as held at the wait, and run it again later.
3. Verify by the guide's step 6. A verify that fails right after the merge is usually timing, because the artifact page lags the tag: run the wait again before calling it a failure.
4. Fill the report's evidence line: `Release: v<version> · published · artifacts verified`, or `failed: <symptom>`.

After a merge on a line, `rk release status` would select the trunk's runs, so wait and verify by `rk guide backport` step 8, which runs the release guide's steps 5 and 6 against the line. Fill the evidence line the same way, prefixed with the line.
