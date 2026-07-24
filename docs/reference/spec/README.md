# vivarium specification

The product contract: what vivarium is and does. These pages are reference material — organized for
lookup, not narrative. They describe the intended design; for what is actually implemented today, see
[`../implementation-status.md`](../implementation-status.md).

The **why** behind these choices lives in the decision records under [`../../decisions/`](../../decisions/);
each page links to the ADRs that govern it rather than restating their reasoning.

## Pages

- [`00-goals-and-non-goals.md`](00-goals-and-non-goals.md) — what vivarium is for, what it is not,
  and who it serves.
- [`01-command-surface.md`](01-command-surface.md) — the command-line verbs, flags, and exit
  behavior.
- [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md) — the four XDG roots, the read-only
  config, and the project→manifest registry in state.
- [`03-artifact-model.md`](03-artifact-model.md) — images, pieces, and manifests, with example
  shapes.
- [`04-composition-and-determinism.md`](04-composition-and-determinism.md) — how layers merge and
  what makes a build reproducible.
- [`05-networking-and-egress.md`](05-networking-and-egress.md) — the network model and the
  open/allowlist egress knob.
- [`06-workspace-and-project-environment.md`](06-workspace-and-project-environment.md) — the mounted
  workspace and the independent inner project environment.
- [`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md) — sharing config across a
  team and injecting secrets safely.
- [`08-invariants-and-guarantees.md`](08-invariants-and-guarantees.md) — the numbered normative
  guarantees the product must uphold.
- [`09-glossary.md`](09-glossary.md) — defined terms used throughout the docs.
