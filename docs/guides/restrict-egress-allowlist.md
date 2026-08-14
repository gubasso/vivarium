# Restrict sandbox egress with an allowlist

Use this flow when a sandbox should reach only named destinations. Author the manifest form from the [artifact model](../reference/spec/03-artifact-model.md), then interpret the evaluated policy using [networking and egress](../reference/spec/05-networking-and-egress.md). Egress is one knob with two modes and it opens by default — [the decision that framed open egress as an exfiltration risk rather than a weaker boundary](../decisions/ADR-0007-default-open-egress.md) explains why restricting it is a deliberate act rather than a correction.

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
$ viv exec -- sh -lc 'fetch-command https://cache.nixos.org/nix-cache-info'
$ viv exec -- sh -lc 'fetch-command https://example.com'
```

Probe one destination the allowlist names and one it omits, and pick both from namespaces that really resolve, so allowlist membership is the only thing that differs — a host that never resolves anywhere would fail identically with no policy in force. Choose a guest command available in the selected image. The denied name fails at resolution: the guest's only nameserver is vivarium's gating resolver, which answers a name no allowlist entry matches with DNS `REFUSED` and never forwards it upstream. A denied literal address fails at connect instead, with a reset.

Expect the denied fetch to fail quickly with an error rather than hang: [networking and egress](../reference/spec/05-networking-and-egress.md) requires a denial to be rejected, not silently dropped, and [the decision placing enforcement on the host side and choosing reject over drop](../decisions/ADR-0044-host-side-egress-and-reject-not-drop.md) records why a hang would be the wrong failure. The exit status you see is the guest command's own, because a started guest command retains its own status boundary — no vivarium code describes a denial. Diagnose vivarium-side failures with the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

Two gated trials in [`user_workflows.rs`](../../tests/user_workflows.rs) cover this: `workflow_05_restrict_egress_config_surface` checks the authoring versus evaluated surfaces and provenance without booting anything, and `workflow_05_restrict_egress_allowlist` checks legible enforcement inside a running guest.
