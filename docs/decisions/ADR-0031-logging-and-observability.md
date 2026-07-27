# ADR-0031: Logging and observability

## Context and Problem Statement

[`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md) said
the state root "holds logs" but left format, level, path, rotation, and flags undefined; ADR-0026
dropped `--log-format`, deferring it to a "future `viv logs` subsystem." A decision is needed now:
where logs go, in what shape, and how the CLI controls them — without a `logs` command.

## Considered Options

- Opt-in logging only (Terraform `TF_LOG` style): no file unless asked.
- Active-by-default file logging with a dedicated `viv logs` reader command.
- Active-by-default file logging, no reader command; docs point to the path; existing `-v`/`-q` plus
  new file-only flags control it.

## Decision Outcome

Chosen option: **active-by-default file logging, no `viv logs` command**.

- **Three faces:** stdout = result only; stderr = human progress/errors (tuned by global `-v`/`-q`);
  a **file** = always-on structured diagnostics — a separate channel, not an echo of stderr.
- **Path:** `${XDG_STATE_HOME:-~/.local/state}/vivarium/logs/vivarium.log` (state class, N12).
- **Flags (file face):** `--log-file`, `--log-level <error|warn|info|debug|trace|off>`,
  `--log-format <logfmt|json>`, `--no-log`. **Env:** `VIV_LOG`, `VIV_LOG_FILE`, `VIV_LOG_FORMAT`
  (not `RUST_LOG` — no impl-stack leak). Precedence flag > env > default, per ADR-0026.
- **Defaults:** file level `info`, format `logfmt` (jsonl opt-in); `-vv`/`-vvv` raise both faces to
  debug/trace; `--log-level` overrides the file floor. Rotation is bounded (an impl detail).
- **Redaction** of secrets/personal paths is deferred to a follow-up ADR.

## Consequences

- Good: a durable diagnostic trail with no UX clutter — the file is invisible unless tailed.
- Good: closes the ADR-0026 `--log-format` deferral without a `logs` command.
- Bad: an always-on writer must degrade gracefully when the state root is unwritable.

## Status

Accepted

Supersedes the `--log-format` deferral in
[`ADR-0026-global-flags-and-config-precedence.md`](ADR-0026-global-flags-and-config-precedence.md).
Applied in [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md),
[`../reference/spec/01-command-surface.md`](../reference/spec/01-command-surface.md), and
[`../reference/spec/02-config-and-xdg-layout.md`](../reference/spec/02-config-and-xdg-layout.md).
