# 16 — Logging and diagnostics

How vivarium writes diagnostics, and how the CLI controls them. The decision and rationale are in [`../../decisions/ADR-0031-logging-and-observability.md`](../../decisions/ADR-0031-logging-and-observability.md); the global-flag taxonomy and the `flag > env > default` precedence it builds on are in [`../../decisions/ADR-0026-global-flags-and-config-precedence.md`](../../decisions/ADR-0026-global-flags-and-config-precedence.md).

## Three faces

Every command's output is split across three channels that never bleed into one another:

- **stdout — the result.** A human table/line, a `--json` record, or nothing for a side-effect command. Pipeable and stable ([`01-command-surface.md`](./01-command-surface.md), ADR-0015).
- **stderr — the human face.** Progress, prompts, warnings, and errors, tuned by the global verbosity flags `-v`/`-q`. Progress shows only when stderr is a TTY.
- **file — the machine/debug face.** A persistent, structured diagnostic log, **written by default**. It is a _separate channel_, not a copy of stderr: it records internal detail (manifest resolution, merge decisions, lifecycle transitions, backend calls, errors) the terminal never shows. It is invisible during normal use — nothing points a user at it unless they raise `-v` or read the file.

An always-on file therefore does **not** clutter the terminal: the UX face stays clean while the log face captures the full trace for post-mortem debugging and bug reports.

## Log file location

The default path is under the **state** root (runtime state that persists across runs — the correct XDG class for logs; [`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md), N12):

```text
${XDG_STATE_HOME:-~/.local/state}/vivarium/logs/vivarium.log
```

`--log-file <path>` (or `VIV_LOG_FILE`) overrides it; `--no-log` disables file logging for the invocation. The writer degrades gracefully — a missing or unwritable state root disables the file face with a single stderr warning rather than failing the command.

## Levels

Six levels, most-to-least severe: `error`, `warn`, `info`, `debug`, `trace` (plus `off`). The **file** and **stderr** faces filter independently:

| Invocation | stderr face                           | file face              |
| ---------- | ------------------------------------- | ---------------------- |
| default    | warnings, errors, and normal progress | `info`                 |
| `-v`       | adds `info`                           | at least `info`        |
| `-vv`      | `debug`                               | `debug`                |
| `-vvv`     | `trace`                               | `trace`                |
| `-q`       | errors only                           | unchanged (still logs) |

`--log-level <level>` (or `VIV_LOG`) sets the **file** floor explicitly and overrides the table above for the file face; `-v`/`-q` continue to tune stderr. The file is a durable audit trail, so it keeps logging even under `-q`.

## Format

`--log-format` (or `VIV_LOG_FORMAT`) selects the file record shape:

- **`logfmt`** (default) — one `key=value` record per line: greppable, tail-able, and readable with no tooling. The right default for a local developer CLI.
- **`json`** — newline-delimited JSON (one object per line) for machine ingestion.

Every record carries at least a timestamp, level, command, and message; structured fields (`project`, `manifest`, `store_path`, `dur_ms`, `err.kind`, …) are added per event. Records are single-line and never ANSI-colored (color is a stderr-only, TTY-only concern).

```text
ts=2026-07-27T19:23:45.123Z level=info cmd=start project=abc msg="build complete" store_path=/nix/store/…-vivarium dur_ms=2500
```

## Rotation

The default log is bounded — it rotates by size and keeps a small number of prior files so it never grows without limit. The exact size and count are an implementation detail, not a stability promise; only the _bounded_ guarantee is normative.

## Flags and environment

All logging settings resolve by the standard precedence **flag > environment variable > default** (ADR-0026):

| Flag                          | Environment          | Default                        |
| ----------------------------- | -------------------- | ------------------------------ |
| `--log-file <path>`           | `VIV_LOG_FILE`       | `…/vivarium/logs/vivarium.log` |
| `--log-level <level>`         | `VIV_LOG`            | `info` (file face)             |
| `--log-format <logfmt\|json>` | `VIV_LOG_FORMAT`     | `logfmt`                       |
| `--no-log`                    | —                    | file logging enabled           |
| `--no-console-log`            | `VIV_NO_CONSOLE_LOG` | guest console captured         |

These are **global** flags — accepted before or after any subcommand, like `-v`/`-q` ([`01-command-surface.md`](./01-command-surface.md)). The environment names are vivarium-specific (`VIV_*`); vivarium never keys logging off an implementation-stack variable such as `RUST_LOG`.

`--no-console-log` is the one row that does not tune the file face: it governs the separate guest-console channel below, which no other flag in this table reaches. `--no-log` does not disable it, and `--log-level` / `--log-format` do not apply to it.

## Redaction

Redaction is **by construction, not by scrubbing**: there is no filter in the write path, no mask list, and no pattern matcher deciding what a record may say. A secret-class value never becomes part of a record in the first place, so there is nothing to strip. Decided in [`../../decisions/ADR-0069-redaction-is-by-construction.md`](../../decisions/ADR-0069-redaction-is-by-construction.md); the secret-handling model it extends is [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md), and the invariant it serves is N10 ([`08-invariants-and-guarantees.md`](./08-invariants-and-guarantees.md)).

### Never the value

For every secret-class channel, vivarium records **names, counts, and shapes — never values**:

```text
ts=… level=debug cmd=start msg="runtime environment injected" env_keys=["CARGO_HOME","GITHUB_TOKEN"] env_count=2
```

The secret-class channels are enumerated, and the list grows additively as new channels are specified:

- manifest `[env]` values and per-invocation `--env` values ([`12-exec-and-shell.md`](./12-exec-and-shell.md));
- the contents of any credential path mounted `readonly = true`;
- the traffic relayed over a forwarded authentication-agent channel ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md)) — that a channel exists and which id it carries are safe to record; not one byte it carries is.

There is deliberately **no encrypted-at-rest entry**, and its absence is a decision rather than an omission: vivarium decrypts nothing and executes no provider, so no plaintext ever reaches a vivarium process to be redacted ([`../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md`](../../decisions/ADR-0072-vivarium-integrates-no-encrypted-at-rest-scheme.md), N25). This is the clearest case of what "by construction" buys — the guarantee needs no enforcement here because there is nothing to enforce it on. Adding a channel that hands vivarium plaintext would convert that structural fact into a claim requiring defense, which is why the refusal list in [`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md) is normative rather than advisory.

Sensitivity **propagates**: a URI, an argument, a header, or an error assembled from a secret is itself secret unless it was built from separately-classified safe parts. Nothing about the verbosity level, the record format, or a panic path relaxes this — `-vvv`, `json`, and an error chain are all subject to the same rule, because the rule is a property of the value and not of the sink.

### Never the whole document

Four things are never written to any channel, at any level: **manifest text**, the **merged configuration**, an **environment snapshot**, and arbitrary structural dumps of internal values. The manifest is compiled into the generated flake and lands in the world-readable store ([`07-secrets-and-config-sharing.md`](./07-secrets-and-config-sharing.md), [`../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md`](../../decisions/ADR-0058-generated-flake-is-a-materialized-cache-artifact.md)), so echoing it into a log compounds a hazard that is already explicit. An unknown or dynamic table is **omitted**, never walked and emitted.

### Generated command lines

The backend and `nix` invocations vivarium constructs are recorded as **structured fields** — executable, argument list, environment key names — and never as a shell-joined string:

- the structured form is `debug`-level;
- a single pasteable string, after the same elision, is `trace`-level only;
- neither appears in `--json`, in `viv doctor`, or on stderr at any verbosity.

Where an argument or environment value is secret-class, the field carries the fixed literal `[REDACTED]`. The marker is always that literal — never a truncated value, a prefix, a length, or a hash, each of which is a side channel and each of which invites a "just show me four characters" regression.

### Personal paths are normalized, not masked

Personal paths are **not** secrets, and blanking them would destroy the diagnostic value of a user's own log for nothing. They are instead normalized in the **persistent log** — `~` for the home directory, `<workspace>` for the project directory, and the durable XDG names for the four config/data/state/cache roots, each keeping its relative suffix. The **stderr and `--json` faces keep exact paths**, because a user acting on a failure needs the path they can actually open ([`14-exit-codes.md`](./14-exit-codes.md)). The `store_path` field remains a normal structured field: a store path is public by construction, and recording it is not a leak.

Containment backs this up rather than replacing it: log files are `0600` under a `0700` state root ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)), and vivarium never uploads a log, never attaches one to anything, and has no command that prints one.

### Failing closed

If a value cannot be classified, or a field cannot be rendered within these rules, the field is **omitted** and the record notes that it was — never serialized raw as a fallback. Losing a field is a diagnostic cost; emitting a secret is an invariant violation.

### The limits, stated plainly

Two things this rule does not reach, said here so no reader over-reads it:

- **Third-party output.** `nix` and the backend write to their own error streams. Vivarium classifies and elides what it _captures_; it cannot guarantee anything about a byte stream produced by another program.
- **The guest console.** `console.log` is outside this section entirely (below) — its bytes are chosen by the guest, and no filter can honestly claim to have sanitized them.

A cheap textual lint, `manifest-no-inline-secret`, warns when a manifest `[env]` value looks like a credential ([`13-doctor-and-health-checks.md`](./13-doctor-and-health-checks.md)). It is **soft and deliberately best-effort** — a heuristic over text, in the same class as `shared-layer-paths-portable` — so a value it fails to flag is not a broken promise. The guarantee is the never-log rule above; the lint only catches a mistake earlier.

## The guest console

The guest's serial console is a **fourth channel**, and deliberately not a fourth face: it is not a copy of stderr, not a structured record stream, and not tuned by `-v`/`-q` or any `--log-*` flag. It exists because when a guest never reaches the agent, the console is the only evidence a boot failure leaves. Decided in [`../../decisions/ADR-0070-guest-console-capture-and-rotation.md`](../../decisions/ADR-0070-guest-console-capture-and-rotation.md).

| Property    | Contract                                                                                                           |
| ----------- | ------------------------------------------------------------------------------------------------------------------ |
| Default     | **captured**, whenever a VM is running                                                                             |
| Destination | `$XDG_RUNTIME_DIR/vivarium/<project-id>/<target>/console.log` ([`12-exec-and-shell.md`](./12-exec-and-shell.md))   |
| Class       | **runtime**, not state — session-scoped, torn down with the runtime root, never swept and never part of history    |
| Content     | **raw guest bytes**, appended byte for byte. No timestamps, no level, no key/value framing, no ANSI interpretation |
| Bound       | rotates by size with a small file count, **per target**                                                            |
| Permissions | `0600`, inside the `0700` runtime root                                                                             |
| Control     | one switch, `--no-console-log` / `VIV_NO_CONSOLE_LOG`                                                              |
| Redaction   | **none, and none claimed**                                                                                         |

Six consequences worth stating:

- **The guest writes to the console device directly.** Guest output does not reach the console by way of the guest's log daemon, which forwards each line with a single best-effort write and never retries a short one — a congested console then truncates a line mid-byte and reports nothing, so the channel would lose exactly the evidence it exists to preserve ([`../../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md`](../../decisions/ADR-0081-guest-console-bypasses-the-guest-log-daemon.md)). The channel is lossless, and a slow reader applies backpressure rather than causing loss. Two limits come with that: the console carries **interleaved writers**, so a line may be split by another writer without any byte being lost, and it carries **no replay** — bytes written while nothing is attached are gone, which is why capture is established before the VM starts rather than when a client asks for it.
- **vivarium owns the file, not the backend.** The console is exposed on a socket that vivarium reads; the backend does not write the log itself. A backend-written file is opened once, truncated, and held for the VM's lifetime, so nothing outside it can rotate the file — a backend-written console is structurally unbounded, which is the one thing this section exists to prevent.
- **Serial, not a virtual console device.** The earliest kernel output — the part that matters when a boot fails — is on the serial line before any other console exists.
- **The bound is a memory bound.** The runtime root is memory-backed ([`02-config-and-xdg-layout.md`](./02-config-and-xdg-layout.md)), and the ceiling multiplies by the number of running VMs, so it is a host-memory cost under N22/N23 ([`17-resources-and-capacity.md`](./17-resources-and-capacity.md)) rather than a disk-space one. It is therefore held tighter than the tool log's.
- **`--attach` reads the same stream.** One reader tees the console to the file and to any attached client, which is what lets `viv start --attach` stream a VM that is already running ([`10-vm-lifecycle.md`](./10-vm-lifecycle.md)).
- **It never propagates.** Because it is unredactable, `console.log` is never copied into the tool log, never surfaced in `--json`, and never included in anything vivarium would assemble for sharing.

Capture is established **before the VM starts**, and failing to establish it fails the launch rather than proceeding. This is deliberately stricter than the tool log, which degrades with a warning when the state root is unwritable: the runtime root is required and never synthesized ([`../../decisions/ADR-0055-runtime-directory-is-required.md`](../../decisions/ADR-0055-runtime-directory-is-required.md)), and silently losing the record of a failed boot defeats the reason the file exists.
