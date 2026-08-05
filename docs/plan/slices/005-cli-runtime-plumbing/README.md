<!-- markdownlint-configure-file {"MD043": {"headings": ["?", "## Goal", "## Appetite", "## Core", "## In scope", "## Out of scope", "## Governed by", "## Acceptance", "## Rabbit holes", "## Done when", "## Revisions"], "match_case": true}} -->

# 005 — CLI runtime plumbing

## Goal

Finish the runtime infrastructure shared by CLI commands once launch and control seams exist.

## Appetite

2 implementation sessions.

## Core

Structured logging uses the chosen non-blocking first-party size-and-count rotation without leaking secrets, and dependency policy is enforceable against the real lockfile.

## In scope

- Implement the rotating writer behind `tracing_appender::non_blocking`.
- Define shutdown, flush, and error semantics.
- Derive `deny.toml` license exceptions from the actual lock.
- Add focused tests and update owning documentation.

## Out of scope

- New CLI verbs.
- Changes to output contracts.
- Generated configuration artifacts.
- Advisory scheduling.

## Governed by

- [`../../../reference/spec/14-exit-codes.md`](../../../reference/spec/14-exit-codes.md) — defines failure categories.
- [`../../../reference/spec/16-logging-and-diagnostics.md`](../../../reference/spec/16-logging-and-diagnostics.md) — defines logging and redaction.
- [`../../../explanation/cli-and-diagnostics.md`](../../../explanation/cli-and-diagnostics.md) — owns the runtime diagnostic topology.
- [`../../../decisions/ADR-0031-logging-and-observability.md`](../../../decisions/ADR-0031-logging-and-observability.md) — fixes file logging.
- [`../../../decisions/ADR-0032-cli-dependency-baseline.md`](../../../decisions/ADR-0032-cli-dependency-baseline.md) — fixes the runtime baseline.
- [`../../../decisions/ADR-0033-error-handling-and-exit-codes.md`](../../../decisions/ADR-0033-error-handling-and-exit-codes.md) — fixes error realization.
- [`../../../decisions/ADR-0034-logging-implementation-and-rotating-writer.md`](../../../decisions/ADR-0034-logging-implementation-and-rotating-writer.md) — fixes the writer design.
- [`../../../decisions/ADR-0069-redaction-is-by-construction.md`](../../../decisions/ADR-0069-redaction-is-by-construction.md) — fixes the sensitive-data boundary.

## Acceptance

When logs reach the size and count bound, the rotating writer SHALL preserve the configured number of files and SHALL continue writing.

If shutdown occurs, then the non-blocking writer SHALL flush according to the chosen boundary.

When dependency policy runs against the lockfile, the policy check SHALL fail on any license exception that is not explicit and justified.

If sensitive values reach an error path, then the diagnostic types SHALL still satisfy redaction by construction.

## Rabbit holes

- Adopting another rotation crate — escape: implement the accepted first-party wrapper.
- A speculative license list — escape: derive exceptions only from the existing lockfile.

## Done when

Every acceptance assertion above holds and is demonstrated by the evidence it names, and the [`milestones.md`](../../milestones.md) row flips to `done`.

## Revisions

None.
