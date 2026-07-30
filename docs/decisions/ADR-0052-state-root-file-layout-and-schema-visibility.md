# ADR-0052: State-root file layout and schema visibility

## Context and Problem Statement

[`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) names a "project registry" and [`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md) an "identity index", but neither says what either is on disk — the file names and shapes existed only as an implementation's private choice. They cannot stay private: [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) has `viv init` print "the exact registry snippet it _would_ record" for a user to paste, so the registry's shape is already half-published, and a snippet nobody has specified is a contract nobody can keep.

## Considered Options

- One combined state file, keyed two ways.
- Two files, both schemas private and unspecified.
- Two files, the registry record public contract and everything else private.

## Decision Outcome

Chosen option: **two files, registry record public** — the rule in [`ADR-0045-config-root-library-layout-and-name-resolution.md`](./ADR-0045-config-root-library-layout-and-name-resolution.md) (what `spec/01` shows the user is contract) makes the pasted record public and leaves the rest private.

- `registry.toml` — TOML array-of-tables; a `[[projects]]` record carries `path` (canonical, symlink-resolved, absolute) and `manifest` (bare kebab-case name). Nothing else.
- `identity.toml` — same format; an `[[identities]]` record carries `id` and `path`.
- Two files rather than one: different key, different write gate, different lifetime.
- **No schema version in either, and unknown keys fail closed.** That is [`ADR-0047-manifest-carries-no-schema-version.md`](./ADR-0047-manifest-carries-no-schema-version.md)'s rule, which reaches these files because the registry record is now something a user writes.
- The `path` and `manifest` keys are a supported interface. Every other state-root file name and shape is unspecified; `viv config --json` and `viv status -g --json` are the supported readers.

## Consequences

- Good: the snippet `spec/01` already promises has a real, documented shape behind it.
- Good: one compatibility rule covers everything a user writes, manifest and registry alike.
- Bad: the two keys are frozen — renaming `manifest` breaks every snippet already pasted.
- Bad: a malformed registry becomes a user-facing config error, so its message is load-bearing.

## Status

Accepted

Specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) and [`../reference/spec/15-project-identity.md`](../reference/spec/15-project-identity.md). Extends the visibility rule of [`ADR-0045-config-root-library-layout-and-name-resolution.md`](./ADR-0045-config-root-library-layout-and-name-resolution.md) to the state root, and the compatibility rule of [`ADR-0047-manifest-carries-no-schema-version.md`](./ADR-0047-manifest-carries-no-schema-version.md) to the registry.
