# ADR-0069: Redaction is by construction, not by scrubbing

## Context and Problem Statement

ADR-0031 deferred redaction of secrets and personal paths, and `spec/16` describes it as a filter applied to records "before they are written" — a framing that presumes a scrubber in the write path. ADR-0058 then made the manifest's own text a store input, so N10 reaches anything echoing a resolved command line or a merged configuration, and the always-on log, `--json`, and stderr are three ways for that to happen.

## Considered Options

- Output filter — scan every record before writing and mask known secret values.
- By construction — a secret value is never formattable or serializable, so no record can contain one; exact-value masking only as defense in depth.
- Opt-in redaction — off by default, enabled by a flag for sharing a log.

## Decision Outcome

Chosen option: by construction.

- Never-log-values. For every secret-class channel vivarium logs names, counts, and shapes — never values. `spec/16` enumerates those channels, additively.
- Types enforce it. Secret-bearing values are wrapped on ingestion in a type whose `Debug` and `Display` render `[REDACTED]` and which is not serializable; exposure is an explicit, greppable call. Sensitivity propagates to anything derived from a secret.
- Command lines are structured, never shell-joined. The resolved backend invocation is a `debug`-level field list; a pasteable single string is `trace`-only, and neither ever reaches `--json`, `doctor`, or stderr.
- Personal paths are normalized, not masked — `~`, `<workspace>`, and the XDG names in the persistent log; exact paths stay on stderr where the user must act on them.
- Fail closed. An unredactable field is omitted, never serialized raw. No verbosity level, output format, or panic path bypasses this.
- The guest console is out of scope — guest-controlled bytes cannot be filtered, and claiming otherwise would be a false guarantee.

## Consequences

- Good: the property is structural, so it cannot be defeated by a new sink or a new format.
- Good: every exposure site is one search away.
- Bad: a heuristic lint over manifest text is best-effort and must be labeled as such.

## Status

Accepted

Discharges the redaction deferral in [`ADR-0031-logging-and-observability.md`](./ADR-0031-logging-and-observability.md) and extends the error-type stack of [`ADR-0033-error-handling-and-exit-codes.md`](./ADR-0033-error-handling-and-exit-codes.md). Specified in [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md), [`../reference/spec/07-secrets-and-config-sharing.md`](../reference/spec/07-secrets-and-config-sharing.md), and [`../reference/spec/13-doctor-and-health-checks.md`](../reference/spec/13-doctor-and-health-checks.md).
