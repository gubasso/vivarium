# Agent Guidelines — vivarium

This file is the cross-tool entry point for AI coding agents. The `AGENTS.md`-native tools (Codex,
Cursor, Copilot, and others) read it, and Claude Code reads the same authored guidance through
[`CLAUDE.md`](CLAUDE.md).

`CLAUDE.md` is the authored source of truth for this project's rules — what vivarium is,
self-contained documentation, the Diátaxis documentation zones, and the lean-ADR discipline. Read it
first and follow it exactly. This file states the non-negotiables agents most often need and defers
the detail to `CLAUDE.md` rather than restating it.

<!-- self-containment -->
## Self-Containment

Non-negotiable: this project is self-contained. The docs describe **only** vivarium, and every fact
must stand alone in this repository. An external reference is allowed only where it names *upstream*
technology that is part of vivarium's own stack (Nix, microvm.nix, NixOS modules, direnv) — never as
a load-bearing dependency on another *project*, tool, external shelf, or personal path. If external
knowledge is required to understand, build, or operate vivarium, copy its essential substance into
the repo. See `CLAUDE.md` § "Self-contained documentation" for the owning statement.

## Decisions

Non-negotiable: record every significant, hard-to-reverse decision as an ADR under `docs/decisions/`,
one decision per file, using the five-section `template.md`. Keep each filled ADR at or below 350
words with exactly one `## Status`. Accepted or implemented ADRs are never deleted — supersede or
reject and link forward. See `CLAUDE.md` § "Documentation Maintenance" for the full rules.

## Working Conventions

- Follow the four Diátaxis documentation zones under `docs/`; keep drafts in the gitignored
  `/.draft/` workspace, out of `docs/`.
- Keep changes scoped and reversible; prefer editing existing files over adding new ones.
- Keep index files free of pasted directory trees — the filesystem owns structure.
- Run the project's own lint and test tasks before proposing changes.
