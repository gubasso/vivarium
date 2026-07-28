# Agent Guidelines — vivarium

This file is the authored source of truth for how AI coding agents work in the vivarium repository. The `AGENTS.md`-native tools (Codex, Cursor, Copilot, and others) read it directly; Claude Code reads the same guidance through a one-line `@AGENTS.md` import in [`CLAUDE.md`](./CLAUDE.md). These instructions override default behavior; follow them exactly.

## What vivarium is

vivarium is a Nix-native tool that boots each project inside its own **microVM** — a separate guest kernel behind a hardware-virtualization boundary — described declaratively in Nix and composed from reusable **images** and **config pieces** unified by a single **manifest**. The command-line tool is a thin wrapper that resolves the manifest, builds the VM with `nix build`, and runs it; the guest kernel, isolation boundary, and configuration-merge semantics are provided by the underlying Nix and virtualization building blocks, not reimplemented here.

The normative rules and guarantees the product must uphold live in [`docs/reference/spec/08-invariants-and-guarantees.md`](docs/reference/spec/08-invariants-and-guarantees.md). Read that file before changing runtime, build, or composition behavior.

<!-- self-containment -->

## Self-Containment

Non-negotiable: this project is self-contained. The docs describe **only** vivarium, and every fact must stand alone in this repository. Do not reference, cite, or link to any other project, tool, or external documentation shelf inside `docs/`, this file, or `README.md`. An external reference is allowed only where it names _upstream_ technology that is part of vivarium's own stack (Nix, microvm.nix, NixOS modules, direnv) — never as a load-bearing dependency on another _project_, tool, external shelf, or personal path. If external knowledge is required to understand, build, or operate vivarium, copy its essential substance into the repo.

## Documentation Maintenance

Documentation uses four Diátaxis zones under `docs/`, each a reader promise:

- `docs/decisions/` — lean architecture decision records (the durable _why_).
- `docs/guides/` — task walkthroughs (the _how-to_).
- `docs/reference/` — exact lookup material, including the product spec under `reference/spec/`.
- `docs/explanation/` — mental models and architecture (the _understanding_).

Rules:

- **Lean ADRs.** Record every significant, hard-to-reverse decision as an ADR under `docs/decisions/`, one decision per file, using the five-section `docs/decisions/template.md`. Keep each filled ADR body at or below **350 words**, with exactly one `## Status` from `Proposed | Accepted | Implemented | Deprecated | Superseded | Rejected`.
- **Never delete** an accepted or implemented decision — but keep old ADRs honest so no reader follows dead rules. Replaced wholesale → mark **Superseded** and link the successor. No longer applicable with no successor → mark **Deprecated** and say why. Changed only in part by a later ADR while the decision still stands → keep the status and add an **"Amended by ADR-NNNN — <what change"** line under `## Status`; edit the old body only where its wording would actively mislead, never to rewrite history.
- **Single source of truth.** Write each durable fact once at its owning home and cross-link with a short reason phrase from everywhere else. Do not restate a fact that another file owns.
- **No pasted trees.** Index files (`README.md`, this file) explain purpose per entry; they never reproduce the directory tree — the filesystem owns structure.
- **Drafts stay out of `docs/`.** Keep scratch material in the gitignored `/.draft/` workspace and promote it into the right zone by rewriting, not moving.
- **Semantic names, stable headings.** Filenames should reveal purpose before the file is opened.
- Update docs only when a change affects durable behavior, operations, or decisions. Small local rationale belongs in load-bearing code comments.

See [`docs/README.md`](docs/README.md) for the zone index and entry points.

## Working Conventions

- Keep changes scoped and reversible; prefer editing existing files over adding new ones.
- Run the project's own lint and test tasks before proposing changes.
