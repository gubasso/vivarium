# KI-0001 upstream escalation

[NixOS/nix#16269](https://github.com/NixOS/nix/issues/16269), filed 2026-08-06. It carries the host reproducer, the `valgrind` origin trace, and a one-line fix at the call site verified against a patched 2.34.8 build.

## Upstream state

Unchanged on 2026-08-13: open, unlabelled, no comments, no reactions, and no pull request fixing it. The uninitialised declaration in `collectGarbage` and both early returns in `LocalOverlayStore::deleteStorePath` are identical at 2.35.0, 2.35.1, 2.35.2, and on `master`, so the defect survived the 2.35 series that released after the report.

Reading the source predicts the recheck rather than performing it. The status stays `open` on the 2026-08-10 host evidence [`investigation.md`](./investigation.md) records, and the resolution condition is still met only by a run.

## Why no pull request yet

The report offers to open one and deliberately has not. It already carries the diff, the patched-build result, and the regression case that turns `tests/functional/local-overlay-store/gc.sh` red once `--max-freed` is passed, so a maintainer who wants the fix loses nothing by the wait. Nothing here depends on the merge either: the mask is `none`, so vivarium has no workaround to retire.

Open the pull request when a maintainer asks for one, when the issue draws a decision on which of the two shapes it offers upstream prefers, or when a pin move puts vivarium on a release where waiting costs more than carrying the patch.
