# Restrict sandbox egress with an allowlist

> **Design-intent walkthrough — not yet working.** This guide describes the _target_ experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when a sandbox should reach only named destinations. Author the manifest form from the [artifact model](../reference/spec/03-artifact-model.md), then interpret the evaluated policy using [networking and egress](../reference/spec/05-networking-and-egress.md).

## Author and inspect the policy

Add the egress declaration to the manifest and any restriction piece needed by the composition. Then inspect the merged result and its provenance:

```console
$ viv config eval --json
$ viv config sources --json
```

The merge behavior is owned by [composition and determinism](../reference/spec/04-composition-and-determinism.md).

## Exercise allowed and denied destinations

```console
$ viv start
$ viv exec -- sh -lc 'fetch-command https://manifest.example'
$ viv exec -- sh -lc 'fetch-command https://blocked.example'
```

Probe one destination the allowlist names and one it omits, and keep both in the same namespace so allowlist membership is the only thing that differs — a host from a namespace that never resolves would fail identically with no policy in force. Choose a guest command available in the selected image. Diagnose vivarium failures with the [exit-code matrix](../reference/spec/14-exit-codes.md); a started guest command retains its own status boundary.

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_05_restrict_egress_config_surface` checks the authoring versus evaluated surfaces and provenance without booting anything, and `workflow_05_restrict_egress_allowlist` checks legible enforcement inside a running guest.
