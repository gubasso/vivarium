# Agent Guidelines — vivarium

This file is the authored source of truth for how AI coding agents work in the vivarium repository. The `AGENTS.md`-native tools (Codex, Cursor, Copilot, and others) read it directly; Claude Code reads the same guidance through a one-line `@AGENTS.md` import in [`CLAUDE.md`](./CLAUDE.md). These instructions override default behavior; follow them exactly.

## What vivarium is

vivarium is a Nix-native tool that boots each project inside its own **microVM** — a separate guest kernel behind a hardware-virtualization boundary — described declaratively in Nix and composed from reusable **images** and **config pieces** unified by a single **manifest**. The command-line tool is a thin wrapper that resolves the manifest, builds the VM with `nix build`, and runs it; the guest kernel, isolation boundary, and configuration-merge semantics are provided by the underlying Nix and virtualization building blocks, not reimplemented here.

The normative rules and guarantees the product must uphold live in [`docs/reference/spec/08-invariants-and-guarantees.md`](docs/reference/spec/08-invariants-and-guarantees.md). Read that file before changing runtime, build, or composition behavior.

<!-- self-containment -->

## Self-Containment and External References

Non-negotiable: the docs describe **only** vivarium, and must make complete sense to a reader who has this repository and a public internet connection — and nothing else. The line is **public versus local**, not internal versus external.

**Forbidden — anything scoped to one person or one machine.** No absolute personal paths (`/home/alice/…`, `~/my-project/`), no reference to a project or working tree that exists only on a contributor's disk, no private notes shelf or personal documentation collection, no URL only the author can reach, and no fact whose justification is "it is in my other repo". A reader who cannot resolve the reference cannot verify the claim, and the reference is therefore worthless to them.

**Allowed and encouraged — the public record.** Upstream technology in vivarium's own stack (Nix, microvm.nix, NixOS modules, direnv), well-known third-party projects, published articles, specifications, standards, CVEs, and their URLs. Cite them properly: name the thing and link it, so a reader can go read the source. A guide that teaches a real workflow should name the real tools that perform it.

**Still required — the substance travels with the citation.** A link is a pointer, not a load-bearing dependency. Where external knowledge is needed to understand, build, or operate vivarium, carry the essential substance in the repo alongside the citation, so a dead link costs a reader convenience rather than a fact. Cite for provenance and depth; never for the part of the explanation that must be here.

## Documentation Maintenance

Documentation uses four Diátaxis zones under `docs/`, each a reader promise:

- `docs/decisions/` — lean architecture decision records (the durable _why_).
- `docs/guides/` — task walkthroughs (the _how-to_).
- `docs/reference/` — exact lookup material, including the product spec under `reference/spec/`.
- `docs/explanation/` — mental models and architecture (the _understanding_).

Rules:

- **Lean ADRs.** Record every significant, hard-to-reverse decision as an ADR under `docs/decisions/`, one decision per file, using the five-section `docs/decisions/template.md`. Keep each filled ADR body at or below **350 words**, with exactly one `## Status` from `Proposed | Accepted | Implemented | Deprecated | Superseded | Rejected`. The cap counts the **body only** — the five decision sections. `## Status` is metadata and is excluded, so the amendment bookkeeping below can accumulate on an old ADR without ever forcing a rewrite of its prose.
- **Never delete** an accepted or implemented decision — but keep old ADRs honest so no reader follows dead rules. Replaced wholesale → mark **Superseded** and link the successor. No longer applicable with no successor → mark **Deprecated** and say why. Changed only in part by a later ADR while the decision still stands → keep the status and add an **"Amended by ADR-NNNN — <what change"** line under `## Status`; edit the old body only where its wording would actively mislead, never to rewrite history.
- **Single source of truth.** Write each durable fact once at its owning home and cross-link with a short reason phrase from everywhere else. Do not restate a fact that another file owns.
- **No pasted trees.** Index files (`README.md`, this file) explain purpose per entry; they never reproduce the directory tree — the filesystem owns structure.
- **Drafts stay out of `docs/`.** Keep scratch material in the gitignored `/.draft/` workspace and promote it into the right zone by rewriting, not moving.
- **Semantic names, stable headings.** Filenames should reveal purpose before the file is opened.
- **The spec outranks the tests, but silence does not.** Acceptance tests encode the spec, so the two can disagree — and when they do, the resolution rule is: a shape a test **deliberately exercises** beats spec **silence**, and an **explicit spec line** beats an unargued assertion in a test helper. Where a test wins, promote the fact into the spec page that owns it in the same change, so the test exercises the contract rather than defining it. Where the spec wins, fix the test. Never leave the pair contradicting.
- Update docs only when a change affects durable behavior, operations, or decisions. Small local rationale belongs in load-bearing code comments.

See [`docs/README.md`](docs/README.md) for the zone index and entry points.

## Working Conventions

- Keep changes scoped and reversible; prefer editing existing files over adding new ones.
- Run the project's own lint and test tasks before proposing changes.
- **The agent's environment is not the target host.** An agent may run in a container, a CI runner, or a sandbox with no `systemd`, no `/dev/kvm`, no session bus, and no `$XDG_RUNTIME_DIR` — none of which says anything about the machines vivarium targets. Never promote an observation of the execution environment into a fact about a host. When such a fact is load-bearing for a decision, mark it unverified in the draft and say so in the write-up until someone confirms it on a real host; a decision may rest on an assumption, but never on an assumption dressed as a measurement.
- **Durable findings belong in this repository, not in agent-private memory.** Anything worth carrying across sessions — a decision, a constraint, a verified premise, a convention — goes to the zone that owns it under `docs/`, or into this file when it is guidance about how to work. Agent-internal memory is not a home for project facts: it is invisible to reviewers, to the other tools that read this file, and to the next contributor.
