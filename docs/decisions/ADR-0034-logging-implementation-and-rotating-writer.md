# ADR-0034: Logging implementation and rotating writer

## Context and Problem Statement

The logging behavior is settled — three faces, an always-on file at `${XDG_STATE_HOME:-~/.local/state}/vivarium/logs/vivarium.log`, `logfmt` default with `json` opt-in, `VIV_LOG*` env, and rotation **bounded by size and file count** ([`ADR-0031-logging-and-observability.md`](./ADR-0031-logging-and-observability.md), [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md)). This ADR fixes the implementation stack, and specifically who enforces the size+count bound.

## Considered Options

- **Framework:** `tracing` + `tracing-subscriber` vs the `log` + `env_logger` stack.
- **logfmt:** the `tracing-logfmt` formatter vs a hand-written `FormatEvent`.
- **Rotation:** `tracing-appender` rolling vs the `file-rotate` crate vs a first-party rotating writer.

## Decision Outcome

Chosen stack: **`tracing` + `tracing-subscriber` + `tracing-logfmt`, with a first-party rotating writer behind `tracing_appender::non_blocking`.**

- **`tracing-subscriber` layers** give the two independent faces directly: a human stderr layer tuned by `-v`/`-q`, and a machine file layer with its own format and level filter. The `VIV_LOG` env name is wired through `EnvFilter::builder().with_env_var("VIV_LOG")`, keeping `RUST_LOG` off the surface.
- **`tracing-logfmt`** provides the default `logfmt` layer; the file layer's `.json()` mode is the opt-in. This avoids hand-maintaining an event formatter.
- **First-party rotating writer.** `tracing-appender` rotates by _time_ only (its `max_log_files` bounds count but nothing bounds size), so it cannot satisfy the size bound spec/16 requires. Rather than take on `file-rotate` — a small, single-maintainer dependency — we implement a `MakeWriter` that rotates on a byte threshold and prunes to a max file count, wrapped in `tracing_appender::non_blocking` for off-thread writes. Exact size/count are an implementation detail per spec/16; only the _bounded_ guarantee is normative.

## Consequences

- Good: satisfies the size+count bound exactly, with no dependency whose rotation model falls short.
- Good: layer composition maps one-to-one onto the three-faces contract.
- Bad: the rotating writer is code we own and must test (threshold, prune, unwritable-root fallback).

## Status

Accepted

Amended by [`ADR-0070-guest-console-capture-and-rotation.md`](./ADR-0070-guest-console-capture-and-rotation.md) — the rotating writer gains a second consumer with its own thresholds and a raw-bytes input path that does not pass through `tracing`. "Exact size/count are an implementation detail" still holds for the tool log; for the guest console the ceiling being small and per-VM is normative, because the runtime root is memory-backed.
