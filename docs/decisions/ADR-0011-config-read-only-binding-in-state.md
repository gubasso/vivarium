# ADR-0011: Config is read-only to the tool; the project binding lives in state

## Context and Problem Statement

Earlier decisions had `viv init` write user config — a registry entry in the config root and
per-project pointer files — while also declaring config the user's hand-authored source of truth.
That is a contradiction: a tool that mutates config as a side effect breaks reproducibility and the
XDG durability split (ADR-0005). We need one coherent rule for who writes what, and one home for the
project→manifest binding.

## Considered Options

- **Keep both sources** — config-root registry plus committed and gitignored repo pointers, with a
  seven-step precedence (the superseded ADR-0006 model).
- **Config read-only; binding in state** — the tool never writes config; the binding is per-user
  runtime state, keyed by project path; repo pointer files are dropped.
- **Prompt on every unbound run** — resolve interactively instead of persisting.

## Decision Outcome

Chosen option: **config read-only; binding in state**.

- **P1 — No runtime config mutation.** vivarium only reads the config root; it never writes, creates,
  or scaffolds there. Everything the tool persists is state, data, or cache (N13).
- **P2 — The only relativization is an explicit, targeted write.** Persisting is gated behind an
  explicit flag naming its target (`viv init --write`), off by default, confirmed, reversible — never
  a silent side effect. In vivarium this write targets **state**, never config.
- The **project registry** (project path → manifest) moves to the state root. Per-project
  `.vivarium.toml` / `.vivarium.local.toml` files are dropped; vivarium writes nothing into a
  project's tree (N9). Precedence collapses to: `--manifest` → `VIVARIUM_MANIFEST` → registry (state)
  → fail closed. The default-manifest fallback is cut.

## Consequences

- Good: config is trustworthy and user-owned; fail-closed stays deterministic; no per-project pointer drift.
- Good: reproducibility comes from tracking the shared library, not scattering pointers.
- Bad: the registry keys on absolute paths, so it is machine-local and not portable (a future evolution).

## Status

Accepted

Supersedes [`ADR-0006-manifest-binding-and-precedence.md`](ADR-0006-manifest-binding-and-precedence.md).
Refines the registry placement of [`ADR-0005-xdg-user-config-layout.md`](ADR-0005-xdg-user-config-layout.md).
