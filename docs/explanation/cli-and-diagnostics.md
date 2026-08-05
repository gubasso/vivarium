# CLI and diagnostics

This page describes the accepted design rather than implemented behavior; see [implementation status](../reference/implementation-status.md) for what runs today.

The CLI is an orchestrator over configuration resolution, Nix evaluation and builds, runtime launch, guest control, and state. Commands share global selection and output conventions while keeping data-shaping flags with the command that owns them. Inspection commands report configuration and state; `doctor` checks prerequisites and health without becoming a second inspection namespace.

Failures map to a program-wide exit taxonomy. Human errors use a fixed rendering skeleton and stable diagnostic identifiers, while structured output provides machine-readable fields. Diagnostic identifiers aid lookup but are not a branch surface; callers branch on documented exit codes. Before 1.0, the command table and acceptance evidence define the compatibility promise, with [implementation status](../reference/implementation-status.md) as the sole current-state owner.

File logging is active by default, structured, bounded by size and count, and separate from user-facing stdout and stderr. Redaction is by construction: sensitive values do not enter diagnostic types or formatted command lines, rather than being scrubbed after formatting. Guest console capture is a distinct stream with its own lifetime and rotation boundary.

Exact commands, fields, codes, checks, and logging values live in the [command surface](../reference/spec/01-command-surface.md), [doctor](../reference/spec/13-doctor-and-health-checks.md), [exit codes](../reference/spec/14-exit-codes.md), and [logging and diagnostics](../reference/spec/16-logging-and-diagnostics.md) specifications.

## Governing decisions

- [ADR-0015](../decisions/ADR-0015-cli-output-and-failure-contract.md) — fixes the stream split and the failure contract every command obeys.
- [ADR-0022](../decisions/ADR-0022-config-inspection-namespace.md) — puts configuration inspection under `viv config` and retires `viv show`.
- [ADR-0023](../decisions/ADR-0023-doctor-check-catalog-and-contract.md) — fixes the `viv doctor` check catalog and its health-report contract.
- [ADR-0026](../decisions/ADR-0026-global-flags-and-config-precedence.md) — fixes the global selection flags and the precedence chain they sit in.
- [ADR-0028](../decisions/ADR-0028-exit-code-taxonomy-and-stability.md) — fixes the program-wide exit categories and their stability promise.
- [ADR-0031](../decisions/ADR-0031-logging-and-observability.md) — fixes that file logging is on by default and separate from user-facing output.
- [ADR-0032](../decisions/ADR-0032-cli-dependency-baseline.md) — fixes the dependency baseline the orchestrator is built on.
- [ADR-0033](../decisions/ADR-0033-error-handling-and-exit-codes.md) — fixes how an error realizes into one of those exit codes.
- [ADR-0034](../decisions/ADR-0034-logging-implementation-and-rotating-writer.md) — fixes the rotating writer that bounds log size and count.
- [ADR-0046](../decisions/ADR-0046-global-config-file-and-precedence.md) — fixes the global config file that the flags override.
- [ADR-0068](../decisions/ADR-0068-human-error-presentation-and-diagnostic-ids.md) — fixes the human error skeleton and the diagnostic identifiers in it.
- [ADR-0070](../decisions/ADR-0070-guest-console-capture-and-rotation.md) — fixes guest console capture as a stream distinct from the log file.
- [ADR-0075](../decisions/ADR-0075-pre-1.0-cli-stability-and-deprecation-policy.md) — fixes what the command table promises before 1.0.

## Unresolved

- Runtime logging and dependency policy belong to [slice 005](../plan/slices/005-cli-runtime-plumbing/README.md).
- Current-state enforcement belongs to [slice 008](../plan/slices/008-enforce-implementation-status/README.md).
