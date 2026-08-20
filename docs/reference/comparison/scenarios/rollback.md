# Rollback

Record whether a build from a past date can be booted again as it was.[^read]

1. Build the environment and record what identifies that build.
2. Change the definition and rebuild.
3. Ask the tool for the earlier environment by that identifier and record what starts.

## vivarium

Specified, not built: `spec/11` fixes per-project generations, each pinned by a GC root, with `viv generations list`/`activate`/`rollback`/`prune` and `viv start --generation <n>`. None of it runs yet — the `*`. No alternative in this set has any answer.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18.
