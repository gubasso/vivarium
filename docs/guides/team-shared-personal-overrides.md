# Combine team configuration with a personal override

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use this flow when a team wants one shared configuration and each person wants their own adjustments on top. The split that makes it work: images and pieces are shared; the manifest is yours ([secrets and configuration sharing](../reference/spec/07-secrets-and-config-sharing.md), settled by [the decision naming the manifest as the personal layer](../decisions/ADR-0040-manifest-is-the-personal-layer.md)). Everyone adopts the same pieces, and nobody edits them to change a value that is only theirs.

## Author the layers

Put the shared image and pieces in the config library ([XDG config-library and registry model](../reference/spec/02-config-and-xdg-layout.md)). A shared piece carries a whole concern — its packages, guest config, mounts, and environment — and references the host only through portable variables like `${HOME}`, which is what keeps it usable on every teammate's machine. That a piece can declare its own mounts and environment at all comes from [the decision giving pieces typed launch-channel options](../decisions/ADR-0021-typed-launch-channel-options-in-pieces.md).

Give each value in a shared piece the priority its role calls for — proposing where your manifest should be free to decide, flooring where a team guarantee must hold for everyone. Which priority carries which role is fixed once in [composition and determinism](../reference/spec/04-composition-and-determinism.md); author against that table rather than against this page, so a change to the convention reaches you here.

Then write your own manifest. It names the image and the pieces to adopt, and carries what is yours alone: resource ceilings, personal mounts, environment values. A teammate writes their own, naming the same pieces with different values. A team can publish an example manifest to start from — you copy and edit it ([generated config examples](../decisions/ADR-0012-generate-config-examples-from-types.md)) — but nothing imports anyone else's.

Your manifest is the personal layer, so a host-specific literal path belongs there and never in a shared piece — [secrets and configuration sharing](../reference/spec/07-secrets-and-config-sharing.md) owns that rule and states how it is enforced.

## Bind and inspect the merge

```console
$ viv init --manifest my-api --write --yes
$ viv config eval --json
$ viv config sources --json
```

Use the evaluated view for the effective result and the sources view for attribution — reach for it especially when two shared pieces collide at the same priority, the case it is designed to survive. What each view emits, and what each returns when the merge carries a defect, belong to the [command surface](../reference/spec/01-command-surface.md) and the [exit-code matrix](../reference/spec/14-exit-codes.md).

## Acceptance coverage

The gated trial [`workflow_03_team_shared_and_personal_override`](../../tests/user_workflows.rs) checks the real config-root model with one shared piece and two different personal manifests, the portable mount value surviving evaluation unexpanded, the winning layer, the shared piece staying byte-identical throughout, the literal-path rejection, and tie reporting.
