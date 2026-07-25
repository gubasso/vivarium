# vivarium documentation

Index into the project's four documentation zones. Each zone is a reader promise, not a topic
bucket. This page is only an index — it holds no canonical facts of its own.

## Zones

- **[`reference/spec/`](reference/spec/README.md)** — the **product contract**: what vivarium is and
  does. Goals and non-goals, the command surface, the XDG config layout, the image/piece/manifest
  artifact model, composition and determinism, networking and egress, the workspace and inner project
  environment, secrets and config sharing, the normative invariants, and the glossary.
- **[`decisions/`](decisions/)** — lean ADRs recording the durable *why* behind the architecture:
  the isolation boundary, the module system as the composition engine, the artifact model, the
  manifest format, the config layout, manifest binding, egress policy, layer separation, workspace
  path handling, secrets, config being read-only to the tool (binding in state), generating config
  examples from types, the VM lifecycle and `viv up` semantics, build generations pinned as GC roots,
  and the CLI output-and-failure contract. `template.md` is the drop-in ADR shape.
- **[`reference/`](reference/)** — lookup material beyond the spec:
  [`implementation-status.md`](reference/implementation-status.md) is the single source of truth for
  what works today versus what is designed only.
- **[`guides/`](guides/)** — task walkthroughs:
  [`getting-started.md`](guides/getting-started.md) walks the intended end-to-end flow.
- **[`explanation/`](explanation/)** — the mental model:
  [`architecture.md`](explanation/architecture.md) explains the sandbox-vs-project-environment split
  and how the pieces fit together.

## Where things live

- The **what** contract: `reference/spec/`.
- The durable **why**: `decisions/`.
- The normative rules the product must uphold:
  [`reference/spec/08-invariants-and-guarantees.md`](reference/spec/08-invariants-and-guarantees.md).
- What actually runs today: [`reference/implementation-status.md`](reference/implementation-status.md).
