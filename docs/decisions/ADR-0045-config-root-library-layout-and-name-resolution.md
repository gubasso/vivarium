# ADR-0045: Config-root library layout and artifact name resolution

## Context and Problem Statement

A manifest names its layers by bare identifier — `image = "rust"`, `pieces = [ "git" ]` — but no document says which file that identifier opens. [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) defers the libraries' shapes to [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md), which shows artifact content and never states a filename convention. The only place the convention is recorded is the acceptance harness, which is exactly the drift [`../reference/implementation-status.md`](../reference/implementation-status.md) warns against. It is not an internal detail either: [`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md) puts the resolved `path` in machine output, so whichever file wins is user-visible contract.

## Considered Options

- Flat files only — `images/rust.nix`.
- Flat first, falling back to a directory — `images/rust.nix`, else `images/rust/default.nix`.
- Directory form only — `images/rust/default.nix`.

## Decision Outcome

Chosen option: flat first, directory as a fallback — flat keeps the common case a one-file edit, and the directory form gives a multi-file artifact somewhere to put its helpers.

- Images and pieces are Nix modules (`.nix`); manifests are TOML (`.toml`). Resolution tries `<library>/<name>.<ext>`, then `<library>/<name>/default.<ext>`.
- A name is kebab-case, `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$` — the identifier spec/01 already promises.
- Both spellings present is an ambiguity, not a precedence puzzle: fail closed naming both paths, which one `mv` resolves.
- Anything in a library directory that is not a member by these rules is invisible to the readers, so an image's own helper modules can sit beside it without becoming listable artifacts.
- The tool only ever reads here (N13); it creates no directory and scaffolds no file.

## Consequences

- Good: images that import one another, which spec/03 already anticipates, gain a home without inventing a second concept.
- Good: the convention moves out of a test file into the spec that owns it.
- Bad: one extra filesystem probe per unresolved name.
- Bad: two legal spellings for one artifact; the ambiguity error is the price of that convenience.

## Status

Accepted

Amended by [`ADR-0063-extends-requires-the-directory-manifest-form.md`](./ADR-0063-extends-requires-the-directory-manifest-form.md) — the directory form stays a fallback everywhere except for a manifest that names `extends`, where it is required so the copied unit excludes the rest of the library. Resolution itself is unchanged.

Amended by [`ADR-0061-examples-ship-not-a-second-namespace.md`](./ADR-0061-examples-ship-not-a-second-namespace.md) — the config root is the whole search path: an artifact bundled with vivarium is an example to copy and never participates in name resolution, so no shadowing rule joins the algorithm above.

Specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) and [`../reference/spec/03-artifact-model.md`](../reference/spec/03-artifact-model.md).
