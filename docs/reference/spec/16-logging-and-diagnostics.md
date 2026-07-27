# 16 — Logging and diagnostics

How vivarium writes diagnostics, and how the CLI controls them. The decision and rationale are in
[`../../decisions/ADR-0031-logging-and-observability.md`](../../decisions/ADR-0031-logging-and-observability.md);
the global-flag taxonomy and the `flag > env > default` precedence it builds on are in
[`../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../decisions/ADR-0026-global-flags-and-config-precedence.md).

## Three faces

Every command's output is split across three channels that never bleed into one another:

- **stdout — the result.** A human table/line, a `--json` record, or nothing for a side-effect
  command. Pipeable and stable ([`01-command-surface.md`](01-command-surface.md), ADR-0015).
- **stderr — the human face.** Progress, prompts, warnings, and errors, tuned by the global
  verbosity flags `-v`/`-q`. Progress shows only when stderr is a TTY.
- **file — the machine/debug face.** A persistent, structured diagnostic log, **written by
  default**. It is a *separate channel*, not a copy of stderr: it records internal detail (manifest
  resolution, merge decisions, lifecycle transitions, backend calls, errors) the terminal never
  shows. It is invisible during normal use — nothing points a user at it unless they raise `-v` or
  read the file.

An always-on file therefore does **not** clutter the terminal: the UX face stays clean while the log
face captures the full trace for post-mortem debugging and bug reports.

## Log file location

The default path is under the **state** root (runtime state that persists across runs — the correct
XDG class for logs; [`02-config-and-xdg-layout.md`](02-config-and-xdg-layout.md), N12):

```text
${XDG_STATE_HOME:-~/.local/state}/vivarium/logs/vivarium.log
```

`--log-file <path>` (or `VIV_LOG_FILE`) overrides it; `--no-log` disables file logging for the
invocation. The writer degrades gracefully — a missing or unwritable state root disables the file
face with a single stderr warning rather than failing the command.

## Levels

Six levels, most-to-least severe: `error`, `warn`, `info`, `debug`, `trace` (plus `off`). The
**file** and **stderr** faces filter independently:

| Invocation | stderr face | file face |
| ---------- | ----------- | --------- |
| default | warnings, errors, and normal progress | `info` |
| `-v` | adds `info` | at least `info` |
| `-vv` | `debug` | `debug` |
| `-vvv` | `trace` | `trace` |
| `-q` | errors only | unchanged (still logs) |

`--log-level <level>` (or `VIV_LOG`) sets the **file** floor explicitly and overrides the table above
for the file face; `-v`/`-q` continue to tune stderr. The file is a durable audit trail, so it keeps
logging even under `-q`.

## Format

`--log-format` (or `VIV_LOG_FORMAT`) selects the file record shape:

- **`logfmt`** (default) — one `key=value` record per line: greppable, tail-able, and readable with
  no tooling. The right default for a local developer CLI.
- **`json`** — newline-delimited JSON (one object per line) for machine ingestion.

Every record carries at least a timestamp, level, command, and message; structured fields
(`project`, `manifest`, `store_path`, `dur_ms`, `err.kind`, …) are added per event. Records are
single-line and never ANSI-colored (color is a stderr-only, TTY-only concern).

```text
ts=2026-07-27T19:23:45.123Z level=info cmd=start project=abc msg="build complete" store_path=/nix/store/…-vivarium dur_ms=2500
```

## Rotation

The default log is bounded — it rotates by size and keeps a small number of prior files so it never
grows without limit. The exact size and count are an implementation detail, not a stability promise;
only the *bounded* guarantee is normative.

## Flags and environment

All logging settings resolve by the standard precedence **flag > environment variable > default**
(ADR-0026):

| Flag | Environment | Default |
| ---- | ----------- | ------- |
| `--log-file <path>` | `VIV_LOG_FILE` | `…/vivarium/logs/vivarium.log` |
| `--log-level <level>` | `VIV_LOG` | `info` (file face) |
| `--log-format <logfmt\|json>` | `VIV_LOG_FORMAT` | `logfmt` |
| `--no-log` | — | file logging enabled |

These are **global** flags — accepted before or after any subcommand, like `-v`/`-q`
([`01-command-surface.md`](01-command-surface.md)). The environment names are vivarium-specific
(`VIV_*`); vivarium never keys logging off an implementation-stack variable such as `RUST_LOG`.

## Redaction

Secrets and personal paths in log records are an open follow-up (a later ADR): redaction, when
specified, applies to records **before** they are written, so no unredacted copy exists on disk. See
[`07-secrets-and-config-sharing.md`](07-secrets-and-config-sharing.md) for the secret-handling model
the redaction rules will extend.
