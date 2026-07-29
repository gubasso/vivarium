# ADR-0042: Evaluation-time content defects are hard errors

## Context and Problem Statement

Two rules the merged configuration must satisfy had no enforcement story. [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) specified an equal-priority tie as exit `0` with a winner "resolved by declaration order" — which the module system does not do and N6 forbids reimplementing. And N11's ban on literal personal paths in shared layers named neither a verb nor a code. Separately, `config sources` was called a pure metadata read though its contract needs per-key winners and priorities, which only the merge produces.

## Considered Options

- Keep the ordered tie by synthesizing priorities from declaration order.
- Make every content defect fail everywhere, `config sources` included.
- One rule: defects are hard errors at evaluation; the provenance viewer renders them without failing.

## Decision Outcome

Chosen option: **one rule** — a merged configuration that vivarium's own rules reject is a hard `65`.

- It applies to `config eval`, `start`, `exec`, and `shell`. Two classes qualify today: an **equal-priority scalar tie** and a **literal personal path in a shared image or piece** (N11).
- **`viv config sources` renders both and exits `0`.** It is the command you reach for _after_ `start` returned `65`; a diagnostic that fails for the reason it exists to explain is useless. ADR-0030 sets the precedent — `status` reports `failed` at exit `0` because state is data.
- Provenance renders from the module system's own **definition list**, not the merged value: on a tie the merge throws and yields nothing to render. Reading definitions is not a merge engine, so N6 holds — but `config sources` does evaluate, so it gains the Nix preflight guard.
- **`65` versus `70`:** `65` is a defect vivarium recognized and named; `70` is a Nix failure it did not anticipate. Recognizing a tie means inspecting definitions before forcing the value.
- Declaration order no longer decides any winner, anywhere.

## Consequences

- Good: N6 holds with no bespoke tiebreak, and one code covers both classes.
- Good: the diagnostic keeps working exactly when evaluation does not.
- Bad: two pieces setting one scalar at normal priority now hard-fail; the `mkDefault`-proposal convention (ADR-0040) is the mitigation.
- Bad: `config sources` needs Nix present.

## Status

Accepted

Amended by **ADR-0049** — a third class qualifies for `65`: a composition that fails one of vivarium's own evaluation-time assertions, such as a guest kernel without the balloon driver or a share naming a cache policy the shipped daemon rejects. One seam is worth stating rather than papering over: these assertions are not per-key merge conflicts, so `viv config sources` surfaces them as check-shaped messages rather than provenance rows.

Amends [`ADR-0002-module-system-as-composition-engine.md`](./ADR-0002-module-system-as-composition-engine.md), [`ADR-0021-typed-launch-channel-options-in-pieces.md`](./ADR-0021-typed-launch-channel-options-in-pieces.md), and [`ADR-0022-config-inspection-namespace.md`](./ADR-0022-config-inspection-namespace.md). Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/04-composition-and-determinism.md`](../reference/spec/04-composition-and-determinism.md), [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md), [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md), and [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md).
