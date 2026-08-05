# ADR-0047: The manifest carries no schema version

## Context and Problem Statement

The manifest is hand-authored TOML, and its compatibility story was left open: whether it declares a `schema_version` and what an unrecognised value would mean. The question is separate from vivarium's output versioning, which [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) already settled — the library and config readers carry no envelope version because per-command stability is the boundary, and the versioned envelope is reserved for `doctor`. That decision governs what vivarium writes; this one governs what a user writes.

## Considered Options

- A required `schema_version` key.
- An optional `schema_version`, with an unrecognised value failing closed.
- No version key; compatibility carried by an additive grammar.

## Decision Outcome

Chosen option: no version key — a hand-authored file's version declaration duplicates what the parser can already see in the key set, and becomes a second, staler source of truth about compatibility than the keys themselves.

- The grammar evolves additively. New keys are optional; a key's meaning is never repurposed.
- Unknown keys fail closed with a message naming the accepted keys and the CLI version, so a manifest written for a newer vivarium is diagnosed rather than half-understood.
- A required version key was rejected because it is a ritual on every file to describe a situation almost no file is in. An optional one was rejected because a key most authors omit cannot be relied on when it matters.
- A genuinely breaking change would signal itself out of band — a new library directory or extension — rather than through a field the incompatible parser must already understand to read.

## Consequences

- Good: the smallest working manifest stays two lines, and nothing in it is bookkeeping.
- Good: one less field that can disagree with the file containing it.
- Bad: an older CLI meeting a newer manifest reports an unknown key rather than "this file is too new", so the quality of that message is load-bearing.
- Bad: no in-band signal is available if a breaking change is ever unavoidable.

## Status

Accepted

Amended by [`ADR-0068-human-error-presentation-and-diagnostic-ids.md`](./ADR-0068-human-error-presentation-and-diagnostic-ids.md) — discharges the load-bearing message named above. The unknown-key message is now normative text in [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md) and must carry the file, the failing position, the unknown key, the accepted key set at that position, and the CLI version.

Amended by [`ADR-0052-state-root-file-layout-and-schema-visibility.md`](./ADR-0052-state-root-file-layout-and-schema-visibility.md) — the scope above ("this one governs what a user writes") now reaches the state registry as well as the manifest, because `viv init`'s pasted snippet makes the registry record something a user writes. No version key and unknown-keys-fail-closed apply there unchanged.

Specified in [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).
