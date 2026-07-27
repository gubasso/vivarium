# ADR-0028: Exit-code taxonomy — program-wide categories and stability

## Context and Problem Statement

ADR-0015 adopted BSD sysexits ("never a generic `1`"), and `exec`/`shell` (ADR-0016) and `doctor`
(ADR-0023) each fixed their own codes. But the code→meaning legend was duplicated across specs, most
commands had no mapping, and nothing stated whether codes are shared or per-command — nor whether
they are stable across releases. Scripts and coding agents need one taxonomy they can branch on.

## Considered Options

- A unique exit code per failure case, namespaced per subcommand.
- Coarse `0`/`1`/`2` only, leaning entirely on stderr.
- A program-wide sysexits category set with a central command × code matrix and an append-only
  stability guarantee.

## Decision Outcome

Chosen option: **program-wide categories + central matrix + append-only stability** — a code names
the *kind* of failure, not the command that raised it.

- **Program-wide.** `78` means config error whether it comes from `start`, `init`, or `config`.
  Commands map their failure conditions onto the fixed set; they never mint private code spaces.
- **Single source of truth.** The legend and the command × code matrix live once, in
  [`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md); every command spec
  describes its *conditions* in prose and links there.
- **Category vs instance.** The code carries the category; stderr's what / where / why / hint carries
  the specifics. This is what keeps the code set small.
- **Append-only stability.** Documented codes are a permanent API: never reassigned, only appended.
  Consumers branch on `0`/non-zero or the documented categories. `126`/`127` stay reserved for future
  not-executable/not-found; the `exec`/`shell` pass-through (`0..255`, `128+S`) is reaffirmed.

## Consequences

- Good: one stable, matchable contract; no duplicated legends; adding a code never breaks a script.
- Good: mapping onto a fixed set, not per-command code design — not cumbersome.
- Bad: coarse codes share meaning; some conditions lean on the message to disambiguate.

## Status

Accepted

Amends [`ADR-0015-cli-output-and-failure-contract.md`](ADR-0015-cli-output-and-failure-contract.md) —
makes the sysexits taxonomy program-wide and adds the central matrix and append-only stability; the
basis is unchanged. Specified in
[`../reference/spec/14-exit-codes.md`](../reference/spec/14-exit-codes.md). Supersedes the retired
ad-hoc `10/20/30/40` sketch.
