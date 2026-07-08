# CLAUDE.md

Project-specific guidance for working in the nixvault repository. These instructions override default
behavior; follow them exactly.

## What nixvault is

nixvault is a Nix-native tool that boots each project inside its own **microVM** — a separate guest
kernel behind a hardware-virtualization boundary — described declaratively in Nix and composed from
reusable **images** and **config pieces** unified by a single **manifest**. The command-line tool is
a thin wrapper that resolves the manifest, builds the VM with `nix build`, and runs it; the guest
kernel, isolation boundary, and configuration-merge semantics are provided by the underlying Nix and
virtualization building blocks, not reimplemented here.

The normative rules and guarantees the product must uphold live in
[`docs/reference/spec/08-invariants-and-guarantees.md`](docs/reference/spec/08-invariants-and-guarantees.md).
Read that file before changing runtime, build, or composition behavior.

## Self-contained documentation

The docs describe **only** nixvault. Do not reference, cite, or link to any other project, tool, or
external documentation shelf inside `docs/`, this file, or `README.md`. Every fact must stand alone
in this repository. External *upstream* technology (Nix, microvm.nix, NixOS modules, direnv) may be
named where it is part of nixvault's own stack; other *projects* may not.

## Documentation Maintenance

Documentation uses four Diátaxis zones under `docs/`, each a reader promise:

- `docs/decisions/` — lean architecture decision records (the durable *why*).
- `docs/guides/` — task walkthroughs (the *how-to*).
- `docs/reference/` — exact lookup material, including the product spec under `reference/spec/`.
- `docs/explanation/` — mental models and architecture (the *understanding*).

Rules:

- **Lean ADRs.** Use the five-section template in `docs/decisions/template.md`. Keep each filled ADR
  body at or below **350 words**, with exactly one `## Status` from
  `Proposed | Accepted | Implemented | Superseded | Rejected`.
- **Never delete** an accepted or implemented decision. Supersede or reject it and link forward.
- **Single source of truth.** Write each durable fact once at its owning home and cross-link with a
  short reason phrase from everywhere else. Do not restate a fact that another file owns.
- **No pasted trees.** Index files (`README.md`, this file) explain purpose per entry; they never
  reproduce the directory tree — the filesystem owns structure.
- **Drafts stay out of `docs/`.** Keep scratch material in the gitignored `/.draft/` workspace and
  promote it into the right zone by rewriting, not moving.
- **Semantic names, stable headings.** Filenames should reveal purpose before the file is opened.
- Update docs only when a change affects durable behavior, operations, or decisions. Small local
  rationale belongs in load-bearing code comments.

## Documentation zone index

See [`docs/README.md`](docs/README.md) for the zone index and entry points.
