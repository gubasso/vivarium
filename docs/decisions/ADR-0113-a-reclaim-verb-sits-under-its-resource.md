# ADR-0113: A reclaim verb sits under its resource, and `viv trim` fans out

## Context and Problem Statement

Two reclaim verbs were named asymmetrically: `viv trim` returns guest memory, `viv volume trim` returns space freed inside a volume. Neither names its resource, and a user who learns one has no reason to guess the other exists. Both are `Designed`, never implemented, so the spelling is still free.

## Considered Options

- Keep `viv trim` and `viv volume trim` unchanged
- A verb namespace: `viv trim <memory|volume>`, with a bare `viv trim` doing both
- Resource namespaces with a fan-out: `viv memory trim`, `viv volume trim`, `viv trim`

## Decision Outcome

Chosen option: resource namespaces with a fan-out — every invocation names its resource without inverting a surface where nouns namespace and verbs are leaves.

`--to <MiB>` stays on `viv memory trim` and `[<name>]` on `viv volume trim`, each on the command whose subject it scopes; the fan-out takes neither.

The fan-out's `--json` nests one subtree per resource, each verbatim the record its own command emits, and carries no top-level total. Host memory bytes and image-allocated bytes are different quantities whose sum names nothing a reader could check.

`viv trim` runs the memory rung first — N23 makes it the only reclaim that exists, while the guest trims volumes anyway — then the volume rung, and it never fails fast. Partial failure takes the rule `viv stop --all` already carries: each failing rung is named on stderr as it fails, the first failure's category is the exit code, and no record is emitted. No new category is minted; ADR-0028 forbids it.

Records written before this one name the verb `viv trim`; their bodies stand.

## Consequences

- Good: a reclaim invocation names its resource, and each command is discoverable from the other.
- Good: no new exit code and no new grammar — the fan-out inherits rules already written.
- Bad: under `--json` a partial fan-out forfeits the measurement it already took, because a failure's record goes to stderr and stdout carries none; the human face streams and loses nothing.
- Bad: the rename reaches the specification, the guides, the comparison pages, and one test.

## Status

Implemented

Enacted 2026-08-27 by slice [`../plan/slices/028-memory-comes-back-without-a-stop/README.md`](../plan/slices/028-memory-comes-back-without-a-stop/README.md): the three verbs run as recorded — flags on the resource commands, the fan-out taking neither, the nested record with no grand total, and the inherited partial-failure rule — demonstrated by `workflow_28_trim_usage_surface`, `workflow_28_memory_trim_reclaims`, and `workflow_28_volume_trim_returns_blocks`.

Amends [`ADR-0035-elastic-guest-memory-model.md`](./ADR-0035-elastic-guest-memory-model.md) — the bounded, user-invoked inflation that record is built around is spelled `viv memory trim`; the elastic model, the rejected controller, and the zero-size balloon are unchanged.

Amends [`ADR-0025-default-hypervisor-cloud-hypervisor.md`](./ADR-0025-default-hypervisor-cloud-hypervisor.md) — only the verb's spelling in its amendment note; the hypervisor choice and the launch profile stand.

Specified in [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md), and [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md). Enacted by [`../plan/slices/028-memory-comes-back-without-a-stop/README.md`](../plan/slices/028-memory-comes-back-without-a-stop/README.md).
