# ADR-0070: Guest console capture, destination, and rotation

## Context and Problem Statement

`spec/12` reserves a `console.log` beside the per-target runtime files and calls it "optional", without saying who writes it, whether it exists by default, or what bounds it. That leaves the principal record of a boot failure undefined: when a guest never reaches the agent, the serial console is the only evidence there is, and today it goes to whatever terminal launched the runner and is lost the moment `start` detaches. `spec/10` also promises `--attach` streams the console, which needs a decided source.

## Considered Options

- Leave it optional and unbounded; point users at the backend's own console flag.
- Have the backend write the file directly (`--serial file=`).
- **vivarium owns the capture**: the backend exposes the console on a socket, vivarium reads it and writes a bounded, rotating file.

## Decision Outcome

Chosen option: **vivarium owns the capture.**

- **On by default**, into `$XDG_RUNTIME_DIR/vivarium/<project-id>/<target>/console.log` — runtime class, session-scoped, torn down with the session, `0600`. One knob disables it.
- **Serial, over a socket.** The console is exposed as a socket the backend listens on and vivarium connects to. Letting the backend open the file directly is rejected: it truncates on open and holds the descriptor for the VM's life, so nothing outside can rotate it — a file-backed console is structurally unbounded.
- **Raw guest bytes.** No timestamps, no level, no structure; it does not participate in `--log-format` and is never merged into the tool log.
- **Bounded and rotating**, per target, using the writer ADR-0034 already requires. The ceiling matters more here than for the tool log because the runtime root is memory-backed, so it is a host-memory cost under N22/N23.
- **Not redacted, and it says so** (ADR-0069). It is therefore never copied into the tool log, `--json`, or any support artifact.
- **One reader serves both** the file and `--attach`.

## Consequences

- Good: boot failures leave evidence that survives detaching.
- Good: `--attach` gains a defined source.
- Bad: capture depends on a host-side reader living as long as the VM — stated here, unverified until the first build.

## Status

Accepted

Amended by **ADR-0081** — the guest writes to the console device directly rather than through its log daemon, which is what makes this channel lossless. The "unverified until the first build" consequence above is **discharged**: a host-side reader attached before the guest's first write does capture the whole stream. Two premises were also corrected on a real host — the console does **not** replay to a late-connecting client, so there is no buffer to bound and the reader must exist from before the guest's first write rather than merely outlive the VM; and a backend-written file was rejected here for the right reason, since the socket the backend listens on is the only form that leaves rotation outside the backend's control.

Extends the three faces of [`ADR-0031-logging-and-observability.md`](./ADR-0031-logging-and-observability.md) with a fourth channel that is deliberately not a face, and gives the writer of [`ADR-0034-logging-implementation-and-rotating-writer.md`](./ADR-0034-logging-implementation-and-rotating-writer.md) a second consumer. Specified in [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md) and [`../reference/spec/12-exec-and-shell.md`](../reference/spec/12-exec-and-shell.md).
