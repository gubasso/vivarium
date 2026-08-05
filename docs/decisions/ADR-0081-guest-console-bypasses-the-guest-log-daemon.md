# ADR-0081: Guest console output bypasses the guest log daemon

## Context and Problem Statement

[`ADR-0070-guest-console-capture-and-rotation.md`](./ADR-0070-guest-console-capture-and-rotation.md) settled how the host captures the guest console, and treated the guest side as uninteresting. It is not. Routing guest output through the guest's log daemon and letting it forward to the console loses bytes: the daemon writes each line to the console with a single best-effort write and never retries a short one, so a congested console silently truncates a line mid-byte and reports nothing. Measured on a real host, with a reader attached for the whole VM life: ~0.1% of a 1 MiB burst gone, no diagnostic anywhere.

## Considered Options

- Keep the log daemon in the path and treat the loss as acceptable noise.
- Bound the loss by rate-limiting guest output.
- The producer writes to the console device directly, and the log daemon keeps only its own copy.

## Decision Outcome

Chosen option: the producer writes to the console device directly.

- The console is a transport, not a log face. Bytes reach it by a blocking write from the process that produced them, so a slow host reader stalls the writer instead of losing data.
- The guest's log daemon still receives its own copy for post-mortem. It is simply not in the path to the console.
- Competing writers are silenced for the duration of any measured burst. The guest init writes status lines to the same console device, and a status line landing mid-write splits a token across two lines — every byte present, the token broken. That is contention, not loss, and the two must never be reported as one thing.
- This is a guest-side contract, so it belongs to the base image rather than to the launch profile.

## Consequences

- Good: the console channel is lossless, which is what makes it usable as evidence.
- Good: removes the daemon's per-line prefix, cutting console traffic ~3.7x for the same payload.
- Bad: guest output no longer carries the daemon's timestamps and unit attribution on the console face.
- Bad: a stalled host reader now applies backpressure to the guest instead of being absorbed.

## Status

Accepted

Amends [`ADR-0070-guest-console-capture-and-rotation.md`](./ADR-0070-guest-console-capture-and-rotation.md), whose "unverified until the first build" consequence this discharges: a host-side reader attached before the guest's first write does capture the whole stream, and the transport is lossless once the guest log daemon is out of it. Specified in [`../reference/spec/16-logging-and-diagnostics.md`](../reference/spec/16-logging-and-diagnostics.md). Enacted by `nix/guest.nix` and proven by `scripts/first-microvm-check` (see [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md)).
