# ADR-0046: The global config file and the configuration precedence chain

## Context and Problem Statement

[`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) says the config root holds one global config file carrying user-wide defaults, and [`ADR-0012-generate-config-examples-from-types.md`](./ADR-0012-generate-config-examples-from-types.md) commits to generating an annotated example for it. But [`ADR-0026-global-flags-and-config-precedence.md`](./ADR-0026-global-flags-and-config-precedence.md) fixed precedence as `flag > environment variable > default` and noted parenthetically that no user config file exists, and no specification page reads a value out of one. The file was asserted in two places and honoured in none.

## Considered Options

- Drop it from this version and edit spec/02 to match ADR-0026.
- Reserve the filename but read nothing from it yet.
- Ship it, and add a config-file tier to the precedence chain.

## Decision Outcome

Chosen option: ship it — a user-wide "I always want this" home is a baseline expectation of a per-user tool, and the two documents already promising it are the ones worth keeping.

- The file is `config.toml` in the config root, hand-authored and read-only to the tool (N13).
- Precedence for cross-cutting concerns becomes flag > environment variable > global config file > built-in default. Today that governs the logging family in [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md); the chain is the standard every later cross-cutting knob joins.
- It does not participate in manifest selection, which stays `--manifest > VIVARIUM_MANIFEST > registry > fail closed` ([`ADR-0011-config-read-only-binding-in-state.md`](./ADR-0011-config-read-only-binding-in-state.md)): a binding is per-project and machine-local, so a user-wide default would be meaningless.
- Colour stays environment-only, preserving the deliberate exception in [`ADR-0015-cli-output-and-failure-contract.md`](./ADR-0015-cli-output-and-failure-contract.md).

## Consequences

- Good: ADR-0012's generator promise becomes real rather than aspirational.
- Good: a setting a user always wants stops needing a shell alias or a wrapper.
- Bad: a fourth tier to document and test at every cross-cutting knob.
- Bad: answering "why is this value what it is" now has four places to look, which raises the bar on `viv config`'s reporting.

## Status

Accepted

Amends [`ADR-0026-global-flags-and-config-precedence.md`](./ADR-0026-global-flags-and-config-precedence.md) — the precedence chain gains a global-config-file tier between environment variables and built-in defaults. Specified in [`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md).
